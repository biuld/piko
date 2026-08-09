pub mod auth;
pub mod executor;
pub mod gateway;
pub mod middleware;
pub mod providers;
pub mod retry;
pub mod stream;
pub mod telemetry;

use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::config::RetryConfig;

/// Build a standard LLM Gateway executor with pre-configured settings.
/// Automatically attaches the default middleware chain (e.g., CostTracker).
pub fn build_gateway(
    providers: HashMap<String, piko_protocol::config::ProviderConfig>,
    retry: RetryConfig,
) -> Arc<dyn crate::gateway::LlmGateway> {
    build_gateway_with_telemetry(providers, retry, Arc::new(telemetry::NoopGatewayTelemetry))
}

/// Like [`build_gateway`], with a hostd-provided telemetry sink for metrics.
pub fn build_gateway_with_telemetry(
    providers: HashMap<String, piko_protocol::config::ProviderConfig>,
    retry: RetryConfig,
    telemetry: Arc<dyn telemetry::GatewayTelemetry>,
) -> Arc<dyn crate::gateway::LlmGateway> {
    let exec = executor::LlmdExecutor::from_providers(providers)
        .with_retry(retry)
        .with_telemetry(telemetry)
        .add_middleware(Arc::new(
            middleware::token_usage::TokenUsageMiddleware::new(),
        ))
        .add_middleware(Arc::new(
            middleware::cost_tracker::CostTrackerMiddleware::new(),
        ));

    Arc::new(exec)
}

pub fn build_gateway_with_auth(
    providers: HashMap<String, piko_protocol::config::ProviderConfig>,
    retry: RetryConfig,
    telemetry: Arc<dyn telemetry::GatewayTelemetry>,
    auth_resolver: Arc<dyn crate::providers::RuntimeAuthResolver>,
) -> Arc<dyn crate::gateway::LlmGateway> {
    let exec = executor::LlmdExecutor::from_providers(providers)
        .with_auth_resolver(auth_resolver)
        .with_retry(retry)
        .with_telemetry(telemetry)
        .add_middleware(Arc::new(
            middleware::token_usage::TokenUsageMiddleware::new(),
        ))
        .add_middleware(Arc::new(
            middleware::cost_tracker::CostTrackerMiddleware::new(),
        ));
    Arc::new(exec)
}
