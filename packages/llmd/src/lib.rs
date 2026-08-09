pub mod auth;
pub mod capabilities;
mod checkpoint;
mod collector;
mod execution;
pub mod executor;
pub mod gateway;
pub mod middleware;
pub mod modeling;
mod protocols;
pub mod providers;
mod redaction;
pub mod retry;
pub mod target;
pub mod telemetry;
pub mod tools;
mod transport;

use std::collections::HashMap;
use std::sync::Arc;

use piko_protocol::config::RetryConfig;

/// Build a standard LLM Gateway executor with pre-configured settings.
/// Automatically attaches the default middleware chain (e.g., CostTracker).
pub fn build_gateway(
    targets: HashMap<String, target::ModelTargetConfig>,
    retry: RetryConfig,
) -> Arc<dyn crate::gateway::InferenceGateway> {
    build_gateway_with_telemetry(targets, retry, Arc::new(telemetry::NoopGatewayTelemetry))
}

/// Like [`build_gateway`], with a hostd-provided telemetry sink for metrics.
pub fn build_gateway_with_telemetry(
    targets: HashMap<String, target::ModelTargetConfig>,
    retry: RetryConfig,
    telemetry: Arc<dyn telemetry::GatewayTelemetry>,
) -> Arc<dyn crate::gateway::InferenceGateway> {
    let exec = executor::LlmdExecutor::from_targets(targets)
        .with_retry(retry)
        .with_telemetry(telemetry)
        .add_middleware(Arc::new(
            middleware::cost_tracker::CostTrackerMiddleware::new(),
        ))
        .add_middleware(Arc::new(
            middleware::token_usage::TokenUsageMiddleware::new(),
        ));

    Arc::new(exec)
}

pub fn build_gateway_with_auth(
    targets: HashMap<String, target::ModelTargetConfig>,
    retry: RetryConfig,
    telemetry: Arc<dyn telemetry::GatewayTelemetry>,
    auth_resolver: Arc<dyn crate::providers::RuntimeAuthResolver>,
) -> Arc<dyn crate::gateway::InferenceGateway> {
    let exec = executor::LlmdExecutor::from_targets(targets)
        .with_auth_resolver(auth_resolver)
        .with_retry(retry)
        .with_telemetry(telemetry)
        .add_middleware(Arc::new(
            middleware::cost_tracker::CostTrackerMiddleware::new(),
        ))
        .add_middleware(Arc::new(
            middleware::token_usage::TokenUsageMiddleware::new(),
        ));
    Arc::new(exec)
}
