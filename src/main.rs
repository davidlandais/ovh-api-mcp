mod auth;
mod sandbox;
mod spec;
mod tools;
mod types;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use auth::OvhClient;
use spec::SpecValidator;
use tools::OvhApiServer;

#[derive(Parser)]
#[command(name = "ovh-api-mcp", about = "OVH API MCP server (Code Mode)")]
struct Cli {
    /// Port to listen on
    #[arg(long, default_value = "3104", env = "PORT")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// OVH API endpoint (eu, ca, us)
    #[arg(long, default_value = "eu", env = "OVH_ENDPOINT")]
    endpoint: String,

    /// OVH application key
    #[arg(long, env = "OVH_APPLICATION_KEY")]
    app_key: Option<String>,

    /// OVH application secret
    #[arg(long, env = "OVH_APPLICATION_SECRET")]
    app_secret: Option<String>,

    /// OVH consumer key
    #[arg(long, env = "OVH_CONSUMER_KEY")]
    consumer_key: Option<String>,

    /// OVH API services to load (comma-separated, or "*" for all)
    #[arg(long, env = "OVH_SERVICES", value_delimiter = ',', default_value = "*")]
    services: Vec<String>,

    /// Directory to cache the merged spec
    #[arg(long, env = "OVH_CACHE_DIR")]
    cache_dir: Option<PathBuf>,

    /// Cache TTL in seconds (0 to disable)
    #[arg(long, env = "OVH_CACHE_TTL", default_value = "86400")]
    cache_ttl: u64,

    /// Disable spec caching entirely
    #[arg(long, default_value = "false")]
    no_cache: bool,

    /// Maximum size of the 'code' field in bytes (default: 1 MiB)
    #[arg(long, env = "OVH_MAX_CODE_SIZE", default_value = "1048576")]
    max_code_size: usize,
}

impl Cli {
    /// Resolve the effective cache directory (explicit flag, or default to $HOME/.cache/ovh-api-mcp/).
    fn effective_cache_dir(&self) -> Option<PathBuf> {
        if self.no_cache || self.cache_ttl == 0 {
            return None;
        }
        if let Some(ref dir) = self.cache_dir {
            return Some(dir.clone());
        }
        // Default: $HOME/.cache/ovh-api-mcp/
        std::env::var("HOME")
            .ok()
            .map(|home| PathBuf::from(home).join(".cache").join("ovh-api-mcp"))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let cli = Cli::parse();

    let cache_dir = cli.effective_cache_dir();
    let cache_ttl = cli.cache_ttl;
    let services = cli.services;
    let max_code_size = cli.max_code_size;

    let has_credentials =
        cli.app_key.is_some() && cli.app_secret.is_some() && cli.consumer_key.is_some();

    let (spec_json, validator, ovh_client) = if has_credentials {
        let ovh_client = OvhClient::new(
            cli.app_key.unwrap(),
            cli.app_secret.unwrap().into(),
            cli.consumer_key.unwrap().into(),
            &cli.endpoint,
        )
        .await?;

        let spec = spec::load_spec(
            ovh_client.base_url(),
            &services,
            cache_dir.as_deref(),
            cache_ttl,
        )
        .await?;

        let spec_json = Arc::new(serde_json::to_string(&spec)?);
        let validator = Arc::new(SpecValidator::from_spec(&spec));
        let ovh_client = Arc::new(ovh_client);

        (Some(spec_json), Some(validator), Some(ovh_client))
    } else {
        tracing::warn!(
            "No OVH credentials provided. Server will start but tools will return errors until credentials are configured."
        );
        (None, None, None)
    };

    let config = StreamableHttpServerConfig::default();
    let cancel_token = config.cancellation_token.clone();

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(OvhApiServer::new(
                spec_json.clone(),
                ovh_client.clone(),
                validator.clone(),
                max_code_size,
            ))
        },
        Arc::new(LocalSessionManager::default()),
        config,
    );

    let app = axum::Router::new().nest_service("/mcp", mcp_service);

    let addr = format!("{}:{}", cli.host, cli.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("ovh-api-mcp server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for Ctrl+C");
            tracing::info!("shutting down");
            cancel_token.cancel();
        })
        .await?;

    Ok(())
}
