use async_trait::async_trait;
use tracing::info;

use crate::middleware::{GatewayContext, LlmdMiddleware};

use crate::gateway::GatewayEvent;

#[derive(Default)]
pub struct CostTrackerMiddleware;

impl CostTrackerMiddleware {
    pub fn new() -> Self {
        Self
    }

    fn calculate_cost(
        model_id: &str,
        usage: &piko_protocol::messages::Usage,
    ) -> piko_protocol::messages::UsageCost {
        let input = usage.input as u32;
        let output = usage.output as u32;
        let cache_read = usage.cache_read as u32;
        let actual_input = input.saturating_sub(cache_read);

        let (in_rate, cache_rate, out_rate) = match model_id {
            "claude-3-5-sonnet-20241022" | "claude-3-5-sonnet-20240620" => (0.003, 0.0003, 0.015),
            "claude-3-5-haiku-20241022" => (0.0008, 0.00008, 0.004),
            "gpt-4o" => (0.0025, 0.00125, 0.010),
            "gpt-4o-mini" => (0.00015, 0.000075, 0.0006),
            _ => (0.0, 0.0, 0.0),
        };

        let input_cost = actual_input as f64 * in_rate / 1000.0;
        let cache_cost = cache_read as f64 * cache_rate / 1000.0;
        let output_cost = output as f64 * out_rate / 1000.0;
        let total = input_cost + cache_cost + output_cost;

        piko_protocol::messages::UsageCost {
            input: input_cost,
            output: output_cost,
            cache_read: cache_cost,
            cache_write: 0.0,
            total,
        }
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
            let cost = Self::calculate_cost(&ctx.model_id, usage);

            usage.cost = cost;

            ctx.metadata
                .insert("cost_usd".to_string(), usage.cost.total.to_string());
            ctx.metadata
                .insert("cost_usd_input".to_string(), usage.cost.input.to_string());
            ctx.metadata
                .insert("cost_usd_output".to_string(), usage.cost.output.to_string());
            ctx.metadata.insert(
                "cost_usd_cache_read".to_string(),
                usage.cost.cache_read.to_string(),
            );

            info!(
                run_id = %ctx.run_id,
                model = %ctx.model_id,
                provider = %ctx.provider,
                cost_usd = usage.cost.total,
                "Cost tracking completed"
            );
        }
        Ok(())
    }
}
