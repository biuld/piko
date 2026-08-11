pub mod cost_tracker;
pub mod token_usage;

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::telemetry::{GatewayTelemetry, NoopGatewayTelemetry};

/// Request-level context passed through the middleware chain
#[derive(Clone, Default)]
pub struct GatewayContext {
    pub run_id: String,
    pub step_id: String,
    pub model_id: String,
    pub provider: String,
    pub api_surface: String,
    pub auth_method: Option<piko_protocol::model::ProviderAuthMethod>,
    pub billing: Option<crate::modeling::BillingPlan>,
    /// Mutable metadata store for middlewares to share data (e.g., costs, trace IDs)
    pub metadata: HashMap<String, String>,
    /// Metrics sink; `None` records nothing (used by tests and no-telemetry
    /// deployments).
    pub telemetry: Option<Arc<dyn GatewayTelemetry>>,
}

impl std::fmt::Debug for GatewayContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayContext")
            .field("run_id", &self.run_id)
            .field("step_id", &self.step_id)
            .field("model_id", &self.model_id)
            .field("provider", &self.provider)
            .field("api_surface", &self.api_surface)
            .field("auth_method", &self.auth_method)
            .field("billing", &self.billing)
            .finish_non_exhaustive()
    }
}

impl GatewayContext {
    pub fn telemetry(&self) -> Arc<dyn GatewayTelemetry> {
        self.telemetry
            .clone()
            .unwrap_or_else(|| Arc::new(NoopGatewayTelemetry))
    }
}

use crate::gateway::{InferenceEvent, InferenceRequest};

/// A filter chain / interceptor hook for LLM requests
#[async_trait]
pub trait LlmdMiddleware: Send + Sync {
    /// Called before the request is sent to the LLM provider.
    /// Return `Err(String)` to short-circuit and abort the request.
    async fn pre_execute(
        &self,
        _ctx: &mut GatewayContext,
        _request: &mut InferenceRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    /// Called when an event is received from the LLM provider stream.
    async fn on_stream_event(
        &self,
        _ctx: &mut GatewayContext,
        _event: &mut InferenceEvent,
    ) -> Result<(), String> {
        Ok(())
    }
}
