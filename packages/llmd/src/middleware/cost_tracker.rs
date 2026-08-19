use std::sync::Arc;

use async_trait::async_trait;
use tracing::{info, warn};

use crate::billing::{BillingContext, BillingRegistry};
use crate::gateway::InferenceEvent;
use crate::middleware::{GatewayContext, LlmdMiddleware};

pub struct CostTrackerMiddleware {
    registry: Arc<BillingRegistry>,
}

impl Default for CostTrackerMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl CostTrackerMiddleware {
    pub fn new() -> Self {
        Self::with_registry(Arc::new(BillingRegistry::standard()))
    }

    pub fn with_registry(registry: Arc<BillingRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl LlmdMiddleware for CostTrackerMiddleware {
    async fn on_stream_event(
        &self,
        ctx: &mut GatewayContext,
        event: &mut InferenceEvent,
    ) -> Result<(), String> {
        let InferenceEvent::Usage(usage) = event else {
            return Ok(());
        };
        usage.cost.entries.clear();
        let Some(plan) = &ctx.billing else {
            return Ok(());
        };
        let billing_context = BillingContext {
            provider: &ctx.provider,
            model: &ctx.model_id,
            api_surface: &ctx.api_surface,
            occurred_at: chrono::Utc::now(),
        };
        usage.cost = match self.registry.estimate(&billing_context, plan, usage) {
            Ok(cost) => cost,
            Err(error) => {
                warn!(
                    target: "llm.cost",
                    provider = %ctx.provider,
                    model = %ctx.model_id,
                    adapter = %plan.usage_adapter,
                    policy = %plan.pricing_policy,
                    error = %error,
                    "llm.cost_unavailable"
                );
                return Ok(());
            }
        };

        if let Some(cost) = usage.cost.entries.first() {
            ctx.metadata
                .insert("cost".to_string(), cost.total.to_string());
            ctx.metadata
                .insert("cost_currency".to_string(), cost.currency.clone());
            ctx.metadata
                .insert("cost_basis".to_string(), cost.basis.as_str().into());
            info!(
                target: "llm.cost",
                run_id = %ctx.run_id,
                step_id = %ctx.step_id,
                model = %ctx.model_id,
                provider = %ctx.provider,
                currency = %cost.currency,
                basis = cost.basis.as_str(),
                cost = cost.total,
                "llm.cost"
            );
        }
        Ok(())
    }
}
