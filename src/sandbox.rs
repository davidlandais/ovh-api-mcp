use std::sync::Arc;

use crate::auth::OvhClient;
use crate::spec::SpecValidator;

/// Maximum memory for the QuickJS runtime (64 MiB).
const JS_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// Maximum stack size for JS execution (1 MiB).
const JS_MAX_STACK_SIZE: usize = 1024 * 1024;
/// Timeout for synchronous JS execution (search tool).
const JS_SYNC_TIMEOUT_SECS: u64 = 10;
/// Timeout for async JS execution (execute tool — includes network I/O).
const JS_ASYNC_TIMEOUT_SECS: u64 = 30;

/// Evaluate a search function in a QuickJS sandbox (sync, no I/O).
///
/// `code` is a JS expression that evaluates to a function: `(spec) => { ... }`.
/// `spec_json` is the OpenAPI spec as a JSON string, exposed as `spec` global.
/// Returns the JSON-stringified result of calling `code()`.
pub fn eval_search(code: &str, spec_json: &str) -> anyhow::Result<String> {
    let rt = rquickjs::Runtime::new().map_err(|e| anyhow::anyhow!("QuickJS runtime error: {e}"))?;

    rt.set_memory_limit(JS_MEMORY_LIMIT);
    rt.set_max_stack_size(JS_MAX_STACK_SIZE);
    rt.set_interrupt_handler(Some({
        let start = std::time::Instant::now();
        Box::new(move || start.elapsed() > std::time::Duration::from_secs(JS_SYNC_TIMEOUT_SECS))
    }));

    let ctx =
        rquickjs::Context::full(&rt).map_err(|e| anyhow::anyhow!("QuickJS context error: {e}"))?;

    ctx.with(|ctx| {
        // Inject the spec JSON as a global string, then parse it in JS
        ctx.globals()
            .set("__specJson", spec_json.to_string())
            .map_err(|e| anyhow::anyhow!("failed to set __specJson: {e}"))?;

        ctx.eval::<(), _>(
            "var spec = JSON.parse(__specJson); delete globalThis.__specJson;".to_string(),
        )
        .map_err(|e| anyhow::anyhow!("failed to parse spec JSON: {e}"))?;

        // Wrap the user code: call the function and JSON.stringify the result
        let wrapped = format!("JSON.stringify(({})(spec))", code);

        let result: String = ctx
            .eval(wrapped)
            .map_err(|e| anyhow::anyhow!("JS eval error: {e}"))?;

        Ok(result)
    })
}

/// Evaluate an execute function in an async QuickJS sandbox with `ovh.request()` bridge.
///
/// `code` is a JS expression that evaluates to an async function: `async () => { ... }`.
/// The function can call `ovh.request({ method, path, query, body })` to make OVH API calls.
/// Each request is validated against the loaded OpenAPI spec.
/// Returns the JSON-stringified result.
pub async fn eval_execute(
    code: &str,
    ovh_client: Arc<OvhClient>,
    validator: Arc<SpecValidator>,
) -> anyhow::Result<String> {
    let rt = rquickjs::AsyncRuntime::new()
        .map_err(|e| anyhow::anyhow!("QuickJS async runtime error: {e}"))?;

    rt.set_memory_limit(JS_MEMORY_LIMIT).await;
    rt.set_max_stack_size(JS_MAX_STACK_SIZE).await;
    rt.set_interrupt_handler(Some({
        let start = std::time::Instant::now();
        Box::new(move || start.elapsed() > std::time::Duration::from_secs(JS_ASYNC_TIMEOUT_SECS))
    }))
    .await;

    let ctx = rquickjs::AsyncContext::full(&rt)
        .await
        .map_err(|e| anyhow::anyhow!("QuickJS async context error: {e}"))?;

    let code = code.to_string();
    let client = ovh_client;

    let result: anyhow::Result<String> = rquickjs::async_with!(ctx => |ctx| {
        // Register __ovh_raw_request(method, path, queryJson, bodyJson) -> string
        // All params and return are strings to avoid JS<->Rust object conversion.
        let client_clone = client.clone();
        let validator_clone = validator.clone();
        let request_fn = rquickjs::Function::new(
            ctx.clone(),
            rquickjs::function::Async(move |method: String, path: String, query_json: String, body_json: String| {
                let c = client_clone.clone();
                let v = validator_clone.clone();
                async move {
                    if !v.is_allowed(&method, &path) {
                        return format!(
                            "__OVH_ERROR__Endpoint {} {} not found in the loaded OpenAPI spec. Use the 'search' tool first to find valid endpoints.",
                            method, path
                        );
                    }

                    if !path.starts_with('/') || path.contains('?') || path.contains('#') || path.contains("..") {
                        return format!(
                            "__OVH_ERROR__Invalid API path: {}. Path must start with '/' and not contain '?', '#' or '..'",
                            path
                        );
                    }

                    let query: Option<serde_json::Value> = if query_json.is_empty() {
                        None
                    } else {
                        serde_json::from_str(&query_json).ok()
                    };
                    let body: Option<serde_json::Value> = if body_json.is_empty() {
                        None
                    } else {
                        serde_json::from_str(&body_json).ok()
                    };
                    match c.request(&method, &path, query.as_ref(), body.as_ref()).await {
                        Ok(v) => serde_json::to_string(&v).unwrap_or_default(),
                        Err(e) => format!("__OVH_ERROR__{}", e),
                    }
                }
            }),
        )
        .map_err(|e| anyhow::anyhow!("failed to create __ovh_raw_request: {e}"))?;

        ctx.globals()
            .set("__ovh_raw_request", request_fn)
            .map_err(|e| anyhow::anyhow!("failed to set __ovh_raw_request: {e}"))?;

        // Define the high-level ovh.request() wrapper in JS
        ctx.eval::<(), _>(
            r#"var ovh = {
                request: async (opts) => {
                    const result = await __ovh_raw_request(
                        opts.method || "GET",
                        opts.path || "",
                        opts.query ? JSON.stringify(opts.query) : "",
                        opts.body ? JSON.stringify(opts.body) : ""
                    );
                    if (result.startsWith("__OVH_ERROR__")) {
                        throw new Error(result.slice(13));
                    }
                    return JSON.parse(result);
                }
            };"#
            .to_string(),
        )
        .map_err(|e| anyhow::anyhow!("failed to define ovh wrapper: {e}"))?;

        // Execute user code wrapped in an async IIFE with try/catch.
        // Errors are returned as "__JS_ERROR__<message>" to preserve the
        // actual exception message (rquickjs loses it on promise rejection).
        let wrapped = format!(
            "(async () => {{ try {{ const __r = await ({})(); return JSON.stringify(__r); }} catch(e) {{ return \"__JS_ERROR__\" + (e.message || String(e)); }} }})()",
            code
        );

        let promise: rquickjs::Promise = ctx
            .eval(wrapped)
            .map_err(|e| anyhow::anyhow!("JS eval error: {e}"))?;

        let result: String = promise
            .into_future::<String>()
            .await
            .map_err(|e| anyhow::anyhow!("JS promise error: {e}"))?;

        if let Some(msg) = result.strip_prefix("__JS_ERROR__") {
            return Err(anyhow::anyhow!("{msg}"));
        }

        Ok(result)
    })
    .await;

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_search_basic() {
        let spec = r#"{"paths":{"/domain/zone":{"get":{"summary":"List zones"}}}}"#;
        let code = r#"(spec) => Object.keys(spec.paths)"#;
        let result = eval_search(code, spec).unwrap();
        assert_eq!(result, r#"["/domain/zone"]"#);
    }

    #[test]
    fn eval_search_filter() {
        let spec = r#"{"paths":{"/domain/zone":{"get":{}},"/ip/block":{"get":{}}}}"#;
        let code = r#"(spec) => Object.keys(spec.paths).filter(p => p.includes("domain"))"#;
        let result = eval_search(code, spec).unwrap();
        assert_eq!(result, r#"["/domain/zone"]"#);
    }

    #[test]
    fn eval_search_error_on_bad_js() {
        let result = eval_search("this is not valid js(", "{}");
        assert!(result.is_err());
    }

    #[test]
    fn eval_search_infinite_loop_is_interrupted() {
        let result = eval_search("(spec) => { while(true) {} }", "{}");
        assert!(
            result.is_err(),
            "infinite loop should be interrupted by timeout"
        );
    }

    #[test]
    fn eval_search_memory_limit() {
        let code = r#"(spec) => { let a = []; for (let i = 0; i < 10000000; i++) a.push(new Array(1000)); return a.length; }"#;
        let result = eval_search(code, "{}");
        assert!(result.is_err(), "excessive memory allocation should fail");
    }

    #[test]
    fn eval_search_stack_overflow() {
        let code = "(spec) => { function f() { return f(); } return f(); }";
        let result = eval_search(code, "{}");
        assert!(result.is_err(), "stack overflow should be caught");
    }
}

#[cfg(test)]
mod validator_tests {
    use crate::spec::SpecValidator;
    use serde_json::json;

    fn test_validator() -> SpecValidator {
        let spec = json!({
            "paths": {
                "/v1/domain/zone": { "get": {}, "post": {} },
                "/v1/domain/zone/{zoneName}": { "get": {}, "put": {}, "delete": {} },
                "/v1/domain/zone/{zoneName}/record/{id}": { "get": {}, "put": {} },
                "/v1/me": { "get": {} },
            }
        });
        SpecValidator::from_spec(&spec)
    }

    #[test]
    fn allows_exact_path_and_method() {
        let v = test_validator();
        assert!(v.is_allowed("GET", "/v1/domain/zone"));
        assert!(v.is_allowed("POST", "/v1/domain/zone"));
        assert!(v.is_allowed("GET", "/v1/me"));
    }

    #[test]
    fn allows_path_with_parameters() {
        let v = test_validator();
        assert!(v.is_allowed("GET", "/v1/domain/zone/example.com"));
        assert!(v.is_allowed("PUT", "/v1/domain/zone/example.com"));
        assert!(v.is_allowed("DELETE", "/v1/domain/zone/example.com"));
    }

    #[test]
    fn allows_nested_parameters() {
        let v = test_validator();
        assert!(v.is_allowed("GET", "/v1/domain/zone/example.com/record/123"));
        assert!(v.is_allowed("PUT", "/v1/domain/zone/example.com/record/456"));
    }

    #[test]
    fn rejects_wrong_method() {
        let v = test_validator();
        assert!(!v.is_allowed("DELETE", "/v1/domain/zone")); // only GET, POST
        assert!(!v.is_allowed("POST", "/v1/me")); // only GET
    }

    #[test]
    fn rejects_unknown_path() {
        let v = test_validator();
        assert!(!v.is_allowed("GET", "/nonexistent"));
        assert!(!v.is_allowed("GET", "/v1/domain/zone/example.com/record/123/extra"));
        assert!(!v.is_allowed("DELETE", "/v1/dedicated/server/ns123.ovh.net"));
    }

    #[test]
    fn case_insensitive_method() {
        let v = test_validator();
        assert!(v.is_allowed("get", "/v1/me"));
        assert!(v.is_allowed("Get", "/v1/me"));
    }

    #[test]
    fn allows_v2_paths() {
        let spec = json!({
            "paths": {
                "/v1/domain/zone": { "get": {} },
                "/v2/iam/policy": { "get": {}, "post": {} },
                "/v2/iam/policy/{policyId}": { "get": {}, "put": {}, "delete": {} },
            }
        });
        let v = SpecValidator::from_spec(&spec);
        assert!(v.is_allowed("GET", "/v1/domain/zone"));
        assert!(v.is_allowed("GET", "/v2/iam/policy"));
        assert!(v.is_allowed("POST", "/v2/iam/policy"));
        assert!(v.is_allowed("GET", "/v2/iam/policy/my-policy-id"));
        assert!(!v.is_allowed("GET", "/v2/nonexistent"));
        assert!(!v.is_allowed("GET", "/v1/iam/policy"));
    }
}
