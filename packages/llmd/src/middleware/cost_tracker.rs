use async_trait::async_trait;
use tracing::info;

use crate::gateway::InferenceEvent;
use crate::middleware::{GatewayContext, LlmdMiddleware};
use crate::modeling::TokenPricing;

#[derive(Default)]
pub struct CostTrackerMiddleware;

impl CostTrackerMiddleware {
    pub fn new() -> Self {
        Self
    }

    fn calculate_cost(
        pricing: &TokenPricing,
        usage: &piko_protocol::messages::Usage,
    ) -> piko_protocol::messages::UsageCostEntry {
        let cache_read = usage.cache_read.min(usage.input);
        let cache_write = usage
            .cache_write
            .min(usage.input.saturating_sub(cache_read));
        let uncached_input = usage
            .input
            .saturating_sub(cache_read)
            .saturating_sub(cache_write);
        let tier = pricing
            .tiers
            .iter()
            .filter(|tier| usage.input > tier.input_tokens_above)
            .max_by_key(|tier| tier.input_tokens_above);
        let input_multiplier = tier.map_or(1.0, |tier| tier.input_multiplier);
        let output_multiplier = tier.map_or(1.0, |tier| tier.output_multiplier);
        let per_million = 1_000_000.0;

        let input_cost =
            uncached_input as f64 * pricing.input_per_million * input_multiplier / per_million;
        let cache_read_cost =
            cache_read as f64 * pricing.cached_input_per_million * input_multiplier / per_million;
        let cache_write_cost = cache_write as f64
            * pricing
                .cache_write_per_million
                .unwrap_or(pricing.input_per_million)
            * input_multiplier
            / per_million;
        let output_cost =
            usage.output as f64 * pricing.output_per_million * output_multiplier / per_million;

        piko_protocol::messages::UsageCostEntry {
            currency: pricing.currency.clone(),
            basis: pricing.basis,
            input: input_cost,
            output: output_cost,
            cache_read: cache_read_cost,
            cache_write: cache_write_cost,
            total: input_cost + cache_read_cost + cache_write_cost + output_cost,
        }
    }
}

#[async_trait]
impl LlmdMiddleware for CostTrackerMiddleware {
    async fn on_stream_event(
        &self,
        ctx: &mut GatewayContext,
        event: &mut InferenceEvent,
    ) -> Result<(), String> {
        if let InferenceEvent::Usage(usage) = event {
            usage.cost.entries = ctx
                .pricing
                .as_ref()
                .map(|pricing| vec![Self::calculate_cost(pricing, usage)])
                .unwrap_or_default();

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
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use piko_protocol::messages::UsageCostBasis;

    use super::*;
    use crate::modeling::TokenPricingTier;

    fn usage(input: u64, output: u64, cache_read: u64, cache_write: u64) -> piko_protocol::Usage {
        piko_protocol::Usage {
            input,
            output,
            cache_read,
            cache_write,
            total_tokens: input + output,
            cost: Default::default(),
        }
    }

    fn pricing(currency: &str, input: f64, cached: f64, output: f64) -> TokenPricing {
        TokenPricing {
            currency: currency.into(),
            basis: UsageCostBasis::ListPrice,
            input_per_million: input,
            cached_input_per_million: cached,
            output_per_million: output,
            cache_write_per_million: None,
            tiers: Vec::new(),
        }
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn provider_schedule_prices_uncached_cached_and_output_tokens() {
        let mut schedule = pricing("USD", 5.0, 0.5, 30.0);
        schedule.cache_write_per_million = Some(6.25);
        let cost = CostTrackerMiddleware::calculate_cost(
            &schedule,
            &usage(1_000_000, 1_000_000, 200_000, 100_000),
        );
        close(cost.input, 3.5);
        close(cost.cache_read, 0.1);
        close(cost.cache_write, 0.625);
        close(cost.output, 30.0);
        close(cost.total, 34.225);
    }

    #[test]
    fn threshold_tier_multiplies_the_whole_request() {
        let mut schedule = pricing("USD", 2.0, 0.2, 12.0);
        schedule.tiers.push(TokenPricingTier {
            input_tokens_above: 272_000,
            input_multiplier: 2.0,
            output_multiplier: 1.5,
        });
        let cost = CostTrackerMiddleware::calculate_cost(&schedule, &usage(272_001, 100_000, 0, 0));
        close(cost.input, 1.088004);
        close(cost.output, 1.8);
        close(cost.total, 2.888004);
    }

    #[test]
    fn deepseek_can_price_in_cny() {
        let cost = CostTrackerMiddleware::calculate_cost(
            &pricing("CNY", 1.0, 0.02, 2.0),
            &usage(1_000_000, 1_000_000, 200_000, 0),
        );
        assert_eq!(cost.currency, "CNY");
        close(cost.total, 2.804);
    }
}
