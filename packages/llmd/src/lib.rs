pub mod auth;
pub mod executor;
pub mod gateway;
pub mod middleware;
pub mod providers;

use std::collections::HashMap;
use std::sync::Arc;

/// Build a standard LLM Gateway executor with pre-configured settings.
/// Automatically attaches the default middleware chain (e.g., CostTracker).
pub fn build_gateway(
    providers: HashMap<String, piko_protocol::config::ProviderConfig>,
) -> Arc<dyn crate::gateway::LlmGateway> {
    let exec = executor::LlmdExecutor::from_providers(providers)
        .add_middleware(Arc::new(middleware::token_usage::TokenUsageMiddleware::new()))
        .add_middleware(Arc::new(middleware::cost_tracker::CostTrackerMiddleware::new()));
    
    Arc::new(exec)
}
