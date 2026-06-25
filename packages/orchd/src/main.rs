// orchd — piko orchestrator daemon
//
// Rust-native AI agent runtime. Communicates with the host-runtime
// (TypeScript) via JSON-RPC over stdio.
//
// Orchestrator is created in "bare" mode (no providers).
// The Host sends configuration via the `configure` RPC method,
// or passes providers at startup via CLI args / env.

use std::collections::HashMap;
use std::sync::Arc;

use orchd::model::executor::SelfLlmExecutor;
use orchd::orchestrator::core::OrchCore;
use orchd::protocol::config::{OrchdConfig, ProviderConfig};
use orchd::rpc::peer::run_stdio_peer;
use tracing::info;
use tracing_subscriber::fmt;

/// Try to build an OrchdConfig from environment variables.
/// Falls back to empty config if no providers are configured.
fn config_from_env() -> OrchdConfig {
    let mut providers = HashMap::new();

    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        let base_url = std::env::var("OPENAI_BASE_URL").ok();
        providers.insert(
            "openai".to_string(),
            ProviderConfig {
                kind: "openai".to_string(),
                api_key: key,
                base_url,
                headers: None,
            },
        );
        info!("configured OpenAI provider from env");
    }

    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        let base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
        providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                kind: "anthropic".to_string(),
                api_key: key,
                base_url,
                headers: None,
            },
        );
        info!("configured Anthropic provider from env");
    }

    if providers.is_empty() {
        info!("no providers configured from env; host must call configure()");
        return OrchdConfig::default();
    }

    // Pick first provider as default
    let first = providers.keys().next().unwrap().clone();
    OrchdConfig {
        providers,
        agents: HashMap::new(),
        default_model: orchd::protocol::config::ModelRef {
            provider: first,
            model_id: "gpt-4o".to_string(),
        },
        ..Default::default()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    fmt::init();

    info!("orchd starting...");
    info!("version: {}", env!("CARGO_PKG_VERSION"));
    info!("edition: Rust 2024");

    // Build config from env (the Host may override via configure() RPC later)
    let config = config_from_env();
    let has_providers = !config.providers.is_empty();

    // Initialize the model executor with provider configs
    let model_executor = Arc::new(SelfLlmExecutor::from_providers(config.providers.clone()));

    // Initialize the orchestrator core
    let orch = OrchCore::from_config(model_executor, config).await;

    if has_providers {
        info!("orchd ready — providers configured, listening on stdio");
    } else {
        info!("orchd ready — no providers yet, waiting for configure() RPC");
    }

    // Run bidirectional JSON-RPC peer (blocks until stdin closes)
    run_stdio_peer(orch).await;

    info!("orchd shutting down");
    Ok(())
}
