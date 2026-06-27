use async_trait::async_trait;
use tracing::info;

use crate::middleware::{GatewayContext, LlmdMiddleware};

use piko_protocol::executor::GatewayEvent;

#[derive(Default)]
pub struct CostTrackerMiddleware;

impl CostTrackerMiddleware {
    pub fn new() -> Self {
        Self
    }


    fn calculate_cost_splits(model_id: &str, input_tokens: u32, output_tokens: u32, cache_read_tokens: Option<u32>) -> (f64, f64, f64, f64) {
        let cache_read = cache_read_tokens.unwrap_or(0);
        let actual_input = input_tokens.saturating_sub(cache_read);

        let (in_rate, cache_rate, out_rate) = match model_id {
            "claude-3-5-sonnet-20241022" | "claude-3-5-sonnet-20240620" => (0.003, 0.0003, 0.015),
            "claude-3-5-haiku-20241022" => (0.0008, 0.00008, 0.004),
            "gpt-4o" => (0.0025, 0.00125, 0.010),
            "gpt-4o-mini" => (0.00015, 0.000075, 0.0006),
            _ => (0.0, 0.0, 0.0),
        };

        let in_cost = actual_input as f64 * in_rate / 1000.0;
        let cache_cost = cache_read as f64 * cache_rate / 1000.0;
        let out_cost = output_tokens as f64 * out_rate / 1000.0;
        let total = in_cost + cache_cost + out_cost;

        (total, in_cost, cache_cost, out_cost)
    }
}

#[async_trait]
impl LlmdMiddleware for CostTrackerMiddleware {
    async fn on_stream_event(
        &self,
        ctx: &mut GatewayContext,
        event: &mut GatewayEvent,
    ) -> Result<(), String> {
        if let GatewayEvent::Usage(usage) = event {
            let (total_cost, in_cost, cache_cost, out_cost) = Self::calculate_cost_splits(
                &ctx.model_id,
                usage.input as u32,
                usage.output as u32,
                Some(usage.cache_read as u32),
            );

            // Populate the struct fields
            usage.cost.total = total_cost;
            usage.cost.input = in_cost;
            usage.cost.output = out_cost;
            usage.cost.cache_read = cache_cost;

            // Record costs to context
            ctx.metadata.insert("cost_usd".to_string(), total_cost.to_string());
            ctx.metadata.insert("cost_usd_input".to_string(), in_cost.to_string());
            ctx.metadata.insert("cost_usd_output".to_string(), out_cost.to_string());
            ctx.metadata.insert("cost_usd_cache_read".to_string(), cache_cost.to_string());

            // Persist / Log telemetry
            info!(
                run_id = %ctx.run_id,
                model = %ctx.model_id,
                provider = %ctx.provider,
                cost_usd = total_cost,
                "Cost tracking completed"
            );
        }
        Ok(())
    }
}
