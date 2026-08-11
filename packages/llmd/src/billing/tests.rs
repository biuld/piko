use std::sync::Arc;

use piko_protocol::messages::{Usage, UsageCost, UsageCostBasis, UsageCostEntry};

use super::*;

struct RequestAdapter;

impl UsageAdapter for RequestAdapter {
    fn id(&self) -> &'static str {
        "test_requests"
    }

    fn adapt(&self, _: &BillingContext<'_>, _: &Usage) -> Result<BillableUsage, String> {
        Ok([("request".into(), 2.0)].into())
    }
}

struct RequestPolicy;

impl PricingPolicy for RequestPolicy {
    fn id(&self) -> &'static str {
        "test_per_request"
    }

    fn validate(&self, _: &BillingPlan) -> Result<(), String> {
        Ok(())
    }

    fn estimate(
        &self,
        _: &BillingContext<'_>,
        plan: &BillingPlan,
        usage: &BillableUsage,
    ) -> Result<UsageCost, String> {
        let amount = usage["request"] * 0.25;
        Ok(UsageCost {
            entries: vec![UsageCostEntry {
                currency: plan.currency.clone(),
                basis: plan.basis,
                components: [("request".into(), amount)].into(),
                total: amount,
            }],
        })
    }
}

#[test]
fn custom_adapter_and_policy_dispatch_without_engine_changes() {
    let mut registry = BillingRegistry::standard();
    registry
        .register_usage_adapter(Arc::new(RequestAdapter))
        .unwrap();
    registry
        .register_pricing_policy(Arc::new(RequestPolicy))
        .unwrap();
    let plan = BillingPlan {
        usage_adapter: "test_requests".into(),
        pricing_policy: "test_per_request".into(),
        currency: "EUR".into(),
        basis: UsageCostBasis::ListPrice,
        configuration: serde_json::json!({}),
    };
    registry.validate(&plan).unwrap();
    let context = BillingContext {
        provider: "custom",
        model: "metered",
        api_surface: "platform",
    };
    let cost = registry.estimate(&context, &plan, &Usage::empty()).unwrap();
    assert_eq!(cost.entries[0].components["request"], 0.5);
}

#[test]
fn duplicate_plugin_ids_are_rejected() {
    let mut registry = BillingRegistry::standard();
    assert!(
        registry
            .register_usage_adapter(Arc::new(RequestAdapter))
            .is_ok()
    );
    assert!(
        registry
            .register_usage_adapter(Arc::new(RequestAdapter))
            .is_err()
    );
}

fn token_plan(currency: &str, input: f64, cached: f64, output: f64) -> BillingPlan {
    BillingPlan {
        usage_adapter: "semantic_tokens".into(),
        pricing_policy: "token_tiered".into(),
        currency: currency.into(),
        basis: UsageCostBasis::ListPrice,
        configuration: serde_json::to_value(StandardTokenPricing {
            input_per_million: input,
            cached_input_per_million: cached,
            output_per_million: output,
            cache_write_per_million: None,
            tiers: Vec::new(),
        })
        .unwrap(),
    }
}

fn usage(input: u64, output: u64, read: u64, write: u64) -> Usage {
    Usage {
        input,
        output,
        cache_read: read,
        cache_write: write,
        total_tokens: input + output,
        ..Default::default()
    }
}

fn context() -> BillingContext<'static> {
    BillingContext {
        provider: "test",
        model: "model",
        api_surface: "platform",
    }
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn standard_policy_prices_token_components_in_native_currency() {
    let registry = BillingRegistry::standard();
    let mut plan = token_plan("CNY", 1.0, 0.02, 2.0);
    let mut config: StandardTokenPricing =
        serde_json::from_value(plan.configuration.clone()).unwrap();
    config.cache_write_per_million = Some(1.25);
    plan.configuration = serde_json::to_value(config).unwrap();
    let cost = registry
        .estimate(
            &context(),
            &plan,
            &usage(1_000_000, 1_000_000, 200_000, 100_000),
        )
        .unwrap();
    let entry = &cost.entries[0];
    assert_eq!(entry.currency, "CNY");
    close(entry.components["input_tokens"], 0.7);
    close(entry.components["cached_input_tokens"], 0.004);
    close(entry.components["cache_write_tokens"], 0.125);
    close(entry.components["output_tokens"], 2.0);
    close(entry.total, 2.829);
}

#[test]
fn standard_policy_applies_long_context_tier() {
    let registry = BillingRegistry::standard();
    let mut plan = token_plan("USD", 2.0, 0.2, 12.0);
    let mut config: StandardTokenPricing =
        serde_json::from_value(plan.configuration.clone()).unwrap();
    config.tiers.push(StandardTokenTier {
        input_tokens_above: 272_000,
        input_multiplier: 2.0,
        output_multiplier: 1.5,
    });
    plan.configuration = serde_json::to_value(config).unwrap();
    let cost = registry
        .estimate(&context(), &plan, &usage(272_001, 100_000, 0, 0))
        .unwrap();
    close(cost.entries[0].components["input_tokens"], 1.088004);
    close(cost.entries[0].components["output_tokens"], 1.8);
}
