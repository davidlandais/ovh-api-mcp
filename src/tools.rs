use std::sync::Arc;

use rmcp::{
    handler::server::router::tool::ToolRouter, handler::server::wrapper::Parameters, model::*,
    tool, tool_handler, tool_router, ServerHandler,
};

use crate::auth::OvhClient;
use crate::sandbox;
use crate::spec::SpecValidator;
use crate::types::CodeInput;

const NO_CREDENTIALS_ERROR: &str =
    "OVH API credentials are not configured. Set OVH_APPLICATION_KEY, OVH_APPLICATION_SECRET, \
     and OVH_CONSUMER_KEY environment variables, then restart the server.";

pub struct OvhApiServer {
    tool_router: ToolRouter<Self>,
    spec_json: Option<Arc<String>>,
    ovh_client: Option<Arc<OvhClient>>,
    validator: Option<Arc<SpecValidator>>,
    max_code_size: usize,
}

impl OvhApiServer {
    pub fn new(
        spec_json: Option<Arc<String>>,
        ovh_client: Option<Arc<OvhClient>>,
        validator: Option<Arc<SpecValidator>>,
        max_code_size: usize,
    ) -> Self {
        Self {
            tool_router: Self::tool_router(),
            spec_json,
            ovh_client,
            validator,
            max_code_size,
        }
    }
}

#[tool_router]
impl OvhApiServer {
    /// Search the OVH API OpenAPI 3.1 spec. All configured services are included. The spec is passed as argument to your function.
    ///
    /// Your code must be a function that receives `spec` and returns a value:
    /// `(spec) => { ... }`
    ///
    /// Types available:
    /// ```typescript
    /// interface ParameterObject {
    ///   name: string; in: "path" | "query"; required: boolean;
    ///   description: string; schema: SchemaObject;
    /// }
    /// interface OperationObject {
    ///   summary: string; parameters: ParameterObject[];
    ///   requestBody?: { content: { "application/json": { schema: SchemaObject } } };
    ///   responses: { "200": { content: { "application/json": { schema: SchemaObject } } } };
    /// }
    /// declare function yourCode(spec: {
    ///   paths: Record<string, Record<string, OperationObject>>;
    ///   components: { schemas: Record<string, SchemaObject> };
    /// }): any;
    /// ```
    ///
    /// Examples:
    /// ```javascript
    /// // Find account-related endpoints
    /// (spec) => {
    ///   const results = [];
    ///   for (const [path, methods] of Object.entries(spec.paths)) {
    ///     if (path.includes("/account")) {
    ///       for (const [method, op] of Object.entries(methods)) {
    ///         results.push({ method: method.toUpperCase(), path, summary: op.summary });
    ///       }
    ///     }
    ///   }
    ///   return results;
    /// }
    /// ```
    /// ```javascript
    /// // Inspect a model schema
    /// (spec) => spec.components.schemas["email.domain.Account"]
    /// ```
    /// ```javascript
    /// // Get request body for creating an account
    /// (spec) => {
    ///   const op = spec.paths["/email/domain/{domain}/account"]?.post;
    ///   return { summary: op?.summary, requestBody: op?.requestBody, parameters: op?.parameters };
    /// }
    /// ```
    #[tool(name = "search")]
    async fn search(
        &self,
        Parameters(input): Parameters<CodeInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let spec = match &self.spec_json {
            Some(s) => s.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    NO_CREDENTIALS_ERROR,
                )]))
            }
        };

        if input.code.len() > self.max_code_size {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Code too large: {} bytes (max {})",
                input.code.len(),
                self.max_code_size
            ))]));
        }

        tracing::info!("search: {}", &input.code[..input.code.len().min(200)]);

        let code = input.code;
        let result = tokio::task::spawn_blocking(move || sandbox::eval_search(&code, &spec))
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("join error: {e}"), None))?;

        match result {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => {
                tracing::warn!("search error: {e}");
                Ok(CallToolResult::error(vec![Content::text(format!("{e}"))]))
            }
        }
    }

    /// Execute JavaScript against the OVH API. Use 'search' first to find endpoints.
    ///
    /// Your code must be an async arrow function: `async () => { ... }`
    ///
    /// Available:
    /// ```typescript
    /// declare const ovh: {
    ///   request(options: {
    ///     method: "GET" | "POST" | "PUT" | "DELETE";
    ///     path: string;
    ///     query?: Record<string, string | number | boolean>;
    ///     body?: unknown;
    ///   }): Promise<any>;
    /// };
    /// ```
    ///
    /// Authentication is automatic. Errors (HTTP >= 400) throw exceptions.
    ///
    /// Examples:
    /// ```javascript
    /// // List email domains
    /// async () => await ovh.request({ method: "GET", path: "/email/domain" })
    /// ```
    /// ```javascript
    /// // List accounts then get details
    /// async () => {
    ///   const accounts = await ovh.request({ method: "GET", path: "/email/domain/example.com/account" });
    ///   const details = [];
    ///   for (const name of accounts.slice(0, 5)) {
    ///     const d = await ovh.request({ method: "GET", path: `/email/domain/example.com/account/${name}` });
    ///     details.push(d);
    ///   }
    ///   return details;
    /// }
    /// ```
    #[tool(name = "execute")]
    async fn execute(
        &self,
        Parameters(input): Parameters<CodeInput>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let client = match &self.ovh_client {
            Some(c) => c.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    NO_CREDENTIALS_ERROR,
                )]))
            }
        };
        let validator = match &self.validator {
            Some(v) => v.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    NO_CREDENTIALS_ERROR,
                )]))
            }
        };

        if input.code.len() > self.max_code_size {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Code too large: {} bytes (max {})",
                input.code.len(),
                self.max_code_size
            ))]));
        }

        tracing::info!("execute: {}", &input.code[..input.code.len().min(200)]);

        let code = input.code.clone();
        let result = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current()
                .block_on(sandbox::eval_execute(&code, client, validator))
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(format!("join error: {e}"), None))?;

        match result {
            Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
            Err(e) => {
                tracing::warn!("execute error: {e}");
                Ok(CallToolResult::error(vec![Content::text(format!("{e}"))]))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for OvhApiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("ovh-api-server", "0.2.0"))
            .with_instructions(
                "MCP server for the OVH API (Code Mode). Two tools: search (explore the \
                 OpenAPI spec with JavaScript) and execute (call the API with JavaScript). \
                 Covers all configured OVH API services. \
                 Authentication is handled transparently.",
            )
    }
}
