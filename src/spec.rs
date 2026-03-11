use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Semaphore;

/// Maximum concurrent HTTP requests when fetching service specs.
const MAX_CONCURRENT_FETCHES: usize = 10;

/// Maximum retry attempts per service fetch.
const MAX_RETRIES: usize = 3;

// ---------------------------------------------------------------------------
// Cache metadata
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct CacheMeta {
    services: Vec<String>,
    created_at: u64,
    ttl_secs: u64,
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Check if a cached spec is still valid. Returns the path to `spec.json` on hit.
fn cache_is_valid(cache_dir: &Path, services: &[String], ttl_secs: u64) -> Option<PathBuf> {
    let meta_path = cache_dir.join("cache-meta.json");
    let spec_path = cache_dir.join("spec.json");

    if !spec_path.exists() || !meta_path.exists() {
        tracing::info!("Cache miss: no cached files in {}", cache_dir.display());
        return None;
    }

    let meta_bytes = std::fs::read(&meta_path).ok()?;
    let meta: CacheMeta = serde_json::from_slice(&meta_bytes).ok()?;

    // Check TTL (use the requested ttl, not the one stored in meta)
    let age = now_epoch().saturating_sub(meta.created_at);
    if age > ttl_secs {
        tracing::info!("Cache miss: expired (age {}s > ttl {}s)", age, ttl_secs);
        return None;
    }

    // Check services match (sorted comparison)
    let mut expected = services.to_vec();
    expected.sort();
    let mut cached = meta.services.clone();
    cached.sort();

    if expected != cached {
        tracing::info!(
            "Cache miss: services changed (cached: {} services, requested: {} services)",
            cached.len(),
            expected.len()
        );
        return None;
    }

    tracing::info!(
        "Cache hit: {} services, age {}s / ttl {}s",
        cached.len(),
        age,
        meta.ttl_secs
    );
    Some(spec_path)
}

/// Write the merged spec and metadata to the cache directory.
fn write_cache(cache_dir: &Path, spec: &Value, services: &[String], ttl_secs: u64) -> Result<()> {
    std::fs::create_dir_all(cache_dir)?;

    let mut sorted_services = services.to_vec();
    sorted_services.sort();

    let meta = CacheMeta {
        services: sorted_services,
        created_at: now_epoch(),
        ttl_secs,
    };

    std::fs::write(cache_dir.join("spec.json"), serde_json::to_string(spec)?)?;
    std::fs::write(
        cache_dir.join("cache-meta.json"),
        serde_json::to_string(&meta)?,
    )?;

    tracing::info!("Cache written to {}", cache_dir.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Service index
// ---------------------------------------------------------------------------

/// Fetch the list of all available OVH API services from the index endpoint.
///
/// The index at `GET {base_url}/` returns `{ "apis": [{ "path": "/domain" }, ...] }`.
pub async fn fetch_service_index(base_url: &str) -> Result<Vec<String>> {
    let url = format!("{}/", base_url.trim_end_matches('/'));
    tracing::info!("Fetching OVH service index from {}", url);

    let resp: Value = reqwest::get(&url).await?.json().await?;

    let services: Vec<String> = resp["apis"]
        .as_array()
        .map(|apis| {
            apis.iter()
                .filter_map(|api| {
                    api["path"]
                        .as_str()
                        .map(|p| p.trim_start_matches('/').to_string())
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    tracing::info!("Service index: {} services found", services.len());
    Ok(services)
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Load the merged OVH OpenAPI spec, with optional caching.
///
/// - If `services` contains `["*"]`, resolves all services via the index.
/// - If `cache_dir` is `Some` and `cache_ttl > 0`, checks disk cache before fetching.
/// - On fetch, tolerates partial failures (logs warnings, continues with available specs).
pub async fn load_spec(
    base_url: &str,
    services: &[String],
    cache_dir: Option<&Path>,
    cache_ttl: u64,
) -> Result<Value> {
    // 1. Resolve wildcard
    let resolved = if services.len() == 1 && services[0] == "*" {
        fetch_service_index(base_url).await?
    } else {
        services.to_vec()
    };

    if resolved.is_empty() {
        bail!("No services to load");
    }

    tracing::info!("Loading {} services", resolved.len());

    // 2. Check cache
    if let Some(dir) = cache_dir {
        if cache_ttl > 0 {
            if let Some(spec_path) = cache_is_valid(dir, &resolved, cache_ttl) {
                let bytes = std::fs::read(&spec_path)?;
                let spec: Value = serde_json::from_slice(&bytes)?;
                return Ok(spec);
            }
        }
    }

    // 3. Fetch and merge (with resilience)
    let spec = fetch_and_merge(base_url, &resolved).await?;

    // 4. Write cache
    if let Some(dir) = cache_dir {
        if cache_ttl > 0 {
            if let Err(e) = write_cache(dir, &spec, &resolved, cache_ttl) {
                tracing::warn!("Failed to write cache: {e}");
            }
        }
    }

    Ok(spec)
}

// ---------------------------------------------------------------------------
// Fetch & merge (with resilience + concurrency limit)
// ---------------------------------------------------------------------------

/// Fetch multiple OVH API specs and merge them into a single OpenAPI 3.1 document.
///
/// Uses a semaphore to limit concurrency. Tolerates partial failures: services that
/// fail to fetch are logged as warnings and skipped.
async fn fetch_and_merge(base_url: &str, services: &[String]) -> Result<Value> {
    let client = reqwest::Client::new();
    let semaphore = std::sync::Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES));
    let mut handles = Vec::new();

    for service in services {
        let base_url = base_url.to_string();
        let service = service.clone();
        let client = client.clone();
        let sem = semaphore.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let result = fetch_with_retry(&client, &base_url, &service).await;
            (service, result)
        }));
    }

    let mut merged_paths = serde_json::Map::new();
    let mut merged_schemas = serde_json::Map::new();
    let total = handles.len();
    let mut failed = 0usize;

    for handle in handles {
        let (service_name, result) = handle.await?;
        match result {
            Ok(spec) => {
                if let Some(paths) = spec["paths"].as_object() {
                    for (k, v) in paths {
                        merged_paths.insert(k.clone(), v.clone());
                    }
                }
                if let Some(schemas) = spec["components"]["schemas"].as_object() {
                    for (k, v) in schemas {
                        merged_schemas.insert(k.clone(), v.clone());
                    }
                }
            }
            Err(e) => {
                failed += 1;
                tracing::warn!("Failed to fetch service '{}': {}", service_name, e);
            }
        }
    }

    let succeeded = total - failed;
    if succeeded == 0 {
        bail!("All {total} services failed to fetch");
    }

    if failed > 0 {
        tracing::warn!(
            "{failed}/{total} services failed, continuing with partial spec ({succeeded} services)"
        );
    }

    tracing::info!(
        "Merged spec: {} paths, {} schemas from {}/{} services",
        merged_paths.len(),
        merged_schemas.len(),
        succeeded,
        total,
    );

    Ok(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "OVH API",
            "version": "1.0",
        },
        "paths": merged_paths,
        "components": {
            "schemas": merged_schemas,
        },
    }))
}

/// Fetch a single service spec with up to MAX_RETRIES attempts.
async fn fetch_with_retry(client: &reqwest::Client, base_url: &str, service: &str) -> Result<Value> {
    let mut last_err = None;
    for attempt in 1..=MAX_RETRIES {
        match fetch_and_convert_with_client(client, base_url, service).await {
            Ok(spec) => return Ok(spec),
            Err(e) => {
                tracing::warn!(
                    "Fetch attempt {}/{} failed for '{}': {}",
                    attempt,
                    MAX_RETRIES,
                    service,
                    e
                );
                last_err = Some(e);
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
                }
            }
        }
    }
    Err(last_err.unwrap())
}

/// Fetch a single OVH API spec and convert it to OpenAPI 3.1 format.
///
/// The OVH spec is fetched from `{base_url}/{service}.json` (public, no auth).
async fn fetch_and_convert_with_client(client: &reqwest::Client, base_url: &str, service: &str) -> Result<Value> {
    let url = format!("{}/{}.json", base_url.trim_end_matches('/'), service);
    tracing::info!("Fetching OVH spec from {}", url);

    let spec: Value = client.get(&url).send().await?.json().await?;

    let api_version = spec["apiVersion"].as_str().unwrap_or("1.0");
    let apis = spec["apis"].as_array();
    let models = spec["models"].as_object();

    // --- Convert paths ---
    let mut paths = serde_json::Map::new();

    if let Some(apis) = apis {
        for api in apis {
            let path = match api["path"].as_str() {
                Some(p) => p,
                None => continue,
            };
            let operations = match api["operations"].as_array() {
                Some(ops) => ops,
                None => continue,
            };

            let path_obj = paths
                .entry(path.to_string())
                .or_insert_with(|| json!({}));

            for op in operations {
                let method = match op["httpMethod"].as_str() {
                    Some(m) => m.to_lowercase(),
                    None => continue,
                };

                let mut operation = serde_json::Map::new();

                if let Some(desc) = op["description"].as_str() {
                    operation.insert("summary".to_string(), json!(desc));
                }

                // noAuthentication
                if op["noAuthentication"].as_bool() == Some(true) {
                    operation.insert("x-no-authentication".to_string(), json!(true));
                }

                // Parameters & body
                let mut params = Vec::new();
                let mut body_props = serde_json::Map::new();
                let mut body_required = Vec::new();

                if let Some(parameters) = op["parameters"].as_array() {
                    for param in parameters {
                        let name = param["name"].as_str().unwrap_or_default();
                        let data_type = param["dataType"].as_str().unwrap_or("string");
                        let param_type = param["paramType"].as_str().unwrap_or("query");
                        let required = param["required"].as_bool().unwrap_or(false);
                        let description = param["description"].as_str().unwrap_or("");

                        match param_type {
                            "path" | "query" => {
                                let location = if param_type == "path" { "path" } else { "query" };
                                let mut p = json!({
                                    "name": name,
                                    "in": location,
                                    "required": required,
                                    "schema": convert_type(data_type),
                                });
                                if !description.is_empty() {
                                    p["description"] = json!(description);
                                }
                                params.push(p);
                            }
                            "body" => {
                                body_props.insert(name.to_string(), {
                                    let mut schema = convert_type(data_type);
                                    if !description.is_empty() {
                                        schema["description"] = json!(description);
                                    }
                                    schema
                                });
                                if required {
                                    body_required.push(json!(name));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !params.is_empty() {
                    operation.insert("parameters".to_string(), json!(params));
                }

                if !body_props.is_empty() {
                    let mut body_schema = json!({
                        "type": "object",
                        "properties": body_props,
                    });
                    if !body_required.is_empty() {
                        body_schema["required"] = json!(body_required);
                    }
                    operation.insert(
                        "requestBody".to_string(),
                        json!({
                            "content": {
                                "application/json": {
                                    "schema": body_schema
                                }
                            }
                        }),
                    );
                }

                // Response
                let response_type = op["responseType"].as_str().unwrap_or("void");
                let response_schema = convert_type(response_type);
                operation.insert(
                    "responses".to_string(),
                    json!({
                        "200": {
                            "description": "Success",
                            "content": {
                                "application/json": {
                                    "schema": response_schema
                                }
                            }
                        }
                    }),
                );

                path_obj[&method] = json!(operation);
            }
        }
    }

    // --- Convert models to components/schemas ---
    let mut schemas = serde_json::Map::new();

    if let Some(models) = models {
        for (model_name, model) in models {
            let schema = if model.get("enum").is_some() {
                // Enum model
                let mut s = json!({
                    "type": model["enumType"].as_str().unwrap_or("string"),
                    "enum": model["enum"],
                });
                if let Some(desc) = model["description"].as_str() {
                    s["description"] = json!(desc);
                }
                s
            } else {
                // Object model
                let mut props = serde_json::Map::new();

                if let Some(properties) = model["properties"].as_object() {
                    for (prop_name, prop) in properties {
                        let prop_type = prop["type"].as_str().unwrap_or("string");
                        let mut schema = convert_type(prop_type);

                        // canBeNull -> nullable via array form
                        if prop["canBeNull"].as_bool() == Some(true) {
                            schema = make_nullable(schema);
                        }

                        if let Some(desc) = prop["description"].as_str() {
                            if !desc.is_empty() {
                                schema["description"] = json!(desc);
                            }
                        }

                        if prop["readOnly"].as_bool() == Some(true) {
                            schema["readOnly"] = json!(true);
                        }

                        props.insert(prop_name.clone(), schema);
                    }
                }

                let mut s = json!({
                    "type": "object",
                    "properties": props,
                });
                if let Some(desc) = model["description"].as_str() {
                    s["description"] = json!(desc);
                }
                s
            };

            schemas.insert(model_name.clone(), schema);
        }
    }

    tracing::info!(
        "Converted {} paths and {} schemas from OVH spec '{}'",
        paths.len(),
        schemas.len(),
        service
    );

    Ok(json!({
        "openapi": "3.1.0",
        "info": {
            "title": format!("OVH API - {}", service),
            "version": api_version,
        },
        "paths": paths,
        "components": {
            "schemas": schemas,
        },
    }))
}

// ---------------------------------------------------------------------------
// Spec validator — path+method enforcement against loaded OpenAPI spec
// ---------------------------------------------------------------------------

/// Segment of an OpenAPI path template.
#[derive(Debug, Clone)]
enum PathSegment {
    /// Literal segment that must match exactly (e.g. "domain").
    Literal(String),
    /// Parameter placeholder (e.g. "{zoneName}"), matches any non-empty segment.
    Param,
}

/// Pre-parsed route: segments + allowed HTTP methods.
#[derive(Debug, Clone)]
struct Route {
    segments: Vec<PathSegment>,
    methods: std::collections::HashSet<String>,
}

/// Validates that a concrete (method, path) pair exists in the loaded OpenAPI spec.
///
/// Handles path-parameter matching: `/domain/zone/{zoneName}` matches `/domain/zone/example.com`.
#[derive(Debug, Clone)]
pub struct SpecValidator {
    routes: Vec<Route>,
}

impl SpecValidator {
    /// Build a validator from the merged OpenAPI spec JSON.
    pub fn from_spec(spec: &Value) -> Self {
        let mut routes = Vec::new();

        if let Some(paths) = spec["paths"].as_object() {
            for (path_template, methods_obj) in paths {
                let segments: Vec<PathSegment> = path_template
                    .split('/')
                    .filter(|s| !s.is_empty())
                    .map(|seg| {
                        if seg.starts_with('{') && seg.ends_with('}') {
                            PathSegment::Param
                        } else {
                            PathSegment::Literal(seg.to_string())
                        }
                    })
                    .collect();

                let methods: std::collections::HashSet<String> = methods_obj
                    .as_object()
                    .map(|obj| obj.keys().map(|k| k.to_uppercase()).collect())
                    .unwrap_or_default();

                if !segments.is_empty() && !methods.is_empty() {
                    routes.push(Route { segments, methods });
                }
            }
        }

        tracing::info!("SpecValidator: {} routes loaded", routes.len());
        Self { routes }
    }

    /// Check if a concrete (method, path) pair matches any route in the spec.
    pub fn is_allowed(&self, method: &str, path: &str) -> bool {
        let concrete_segments: Vec<&str> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let method_upper = method.to_uppercase();

        self.routes.iter().any(|route| {
            route.methods.contains(&method_upper)
                && route.segments.len() == concrete_segments.len()
                && route
                    .segments
                    .iter()
                    .zip(concrete_segments.iter())
                    .all(|(tmpl, concrete)| match tmpl {
                        PathSegment::Param => !concrete.is_empty(),
                        PathSegment::Literal(lit) => lit == concrete,
                    })
        })
    }
}

// ---------------------------------------------------------------------------
// Type conversion helpers
// ---------------------------------------------------------------------------

/// Convert an OVH type string to an OpenAPI 3.1 schema fragment.
fn convert_type(data_type: &str) -> Value {
    // Array types: "string[]", "long[]", etc.
    if let Some(inner) = data_type.strip_suffix("[]") {
        return json!({
            "type": "array",
            "items": convert_type(inner),
        });
    }

    // Map types: "map[string]string", etc.
    if data_type.starts_with("map[") {
        return json!({ "type": "object" });
    }

    match data_type {
        "string" | "text" | "password" | "uuid" | "ip" | "ipv4" | "ipv6" | "ipBlock" => {
            json!({ "type": "string" })
        }
        "long" => json!({ "type": "integer" }),
        "boolean" => json!({ "type": "boolean" }),
        "double" => json!({ "type": "number" }),
        "datetime" => json!({ "type": "string", "format": "date-time" }),
        "date" => json!({ "type": "string", "format": "date" }),
        "void" => json!({ "type": "null" }),
        // Anything else is a $ref to a model
        other => json!({ "$ref": format!("#/components/schemas/{}", other) }),
    }
}

/// Turn a schema into its nullable OpenAPI 3.1 variant.
///
/// If the schema has a simple `"type": "string"`, produce `"type": ["string", "null"]`.
/// If it is a `$ref`, wrap with anyOf.
fn make_nullable(mut schema: Value) -> Value {
    if schema.get("$ref").is_some() {
        // $ref cannot be combined with type; use anyOf
        return json!({
            "anyOf": [schema, { "type": "null" }]
        });
    }

    if let Some(t) = schema.get("type").cloned() {
        match t {
            Value::String(s) if s != "null" => {
                schema["type"] = json!([s, "null"]);
            }
            _ => {}
        }
    }
    schema
}
