use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{BillableUsage, BillingContext, PricingPolicy, UsageAdapter};
use crate::modeling::BillingPlan;

pub(super) struct SemanticTokenAdapter;

impl UsageAdapter for SemanticTokenAdapter {
    fn id(&self) -> &'static str {
        "semantic_tokens"
    }

    fn adapt(
        &self,
        _context: &BillingContext<'_>,
        usage: &piko_protocol::messages::Usage,
    ) -> Result<BillableUsage, String> {
        let cache_read = usage.cache_read.min(usage.input);
        let cache_write = usage
            .cache_write
            .min(usage.input.saturating_sub(cache_read));
        let uncached = usage
            .input
            .saturating_sub(cache_read)
            .saturating_sub(cache_write);
        let mut result = usage.units.clone();
        result.insert("input_tokens".into(), uncached as f64);
        result.insert("cached_input_tokens".into(), cache_read as f64);
        result.insert("cache_write_tokens".into(), cache_write as f64);
        result.insert("output_tokens".into(), usage.output as f64);
        result.insert("total_input_tokens".into(), usage.input as f64);
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardTokenPricing {
    pub input_per_million: f64,
    pub cached_input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: Option<f64>,
    #[serde(default)]
    pub tiers: Vec<StandardTokenTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardTokenTier {
    pub input_tokens_above: u64,
    pub input_multiplier: f64,
    pub output_multiplier: f64,
}

pub(super) struct TokenTieredPolicy;

impl PricingPolicy for TokenTieredPolicy {
    fn id(&self) -> &'static str {
        "token_tiered"
    }

    fn validate(&self, plan: &BillingPlan) -> Result<(), String> {
        let schedule = schedule(plan)?;
        validate_schedule(&schedule)
    }

    fn estimate(
        &self,
        _context: &BillingContext<'_>,
        plan: &BillingPlan,
        usage: &BillableUsage,
    ) -> Result<piko_protocol::messages::UsageCost, String> {
        let schedule = schedule(plan)?;
        estimate_standard(&plan.currency, plan.basis, &schedule, usage)
    }
}

pub(super) fn validate_schedule(schedule: &StandardTokenPricing) -> Result<(), String> {
    let rates = [
        schedule.input_per_million,
        schedule.cached_input_per_million,
        schedule.output_per_million,
        schedule
            .cache_write_per_million
            .unwrap_or(schedule.input_per_million),
    ];
    if rates.iter().any(|rate| !rate.is_finite() || *rate < 0.0) {
        return Err("Token pricing contains an invalid rate".into());
    }
    if schedule.tiers.iter().any(|tier| {
        !tier.input_multiplier.is_finite()
            || tier.input_multiplier <= 0.0
            || !tier.output_multiplier.is_finite()
            || tier.output_multiplier <= 0.0
    }) {
        return Err("Token pricing contains an invalid tier".into());
    }
    Ok(())
}

pub(super) fn estimate_standard(
    currency: &str,
    basis: piko_protocol::messages::UsageCostBasis,
    schedule: &StandardTokenPricing,
    usage: &BillableUsage,
) -> Result<piko_protocol::messages::UsageCost, String> {
    let total_input = metric(usage, "total_input_tokens")?;
    let tier = schedule
        .tiers
        .iter()
        .filter(|tier| total_input > tier.input_tokens_above as f64)
        .max_by_key(|tier| tier.input_tokens_above);
    let input_multiplier = tier.map_or(1.0, |tier| tier.input_multiplier);
    let output_multiplier = tier.map_or(1.0, |tier| tier.output_multiplier);
    let per_million = 1_000_000.0;
    let mut components = BTreeMap::new();
    components.insert(
        "input_tokens".into(),
        metric(usage, "input_tokens")? * schedule.input_per_million * input_multiplier
            / per_million,
    );
    components.insert(
        "cached_input_tokens".into(),
        metric(usage, "cached_input_tokens")?
            * schedule.cached_input_per_million
            * input_multiplier
            / per_million,
    );
    components.insert(
        "cache_write_tokens".into(),
        metric(usage, "cache_write_tokens")?
            * schedule
                .cache_write_per_million
                .unwrap_or(schedule.input_per_million)
            * input_multiplier
            / per_million,
    );
    components.insert(
        "output_tokens".into(),
        metric(usage, "output_tokens")? * schedule.output_per_million * output_multiplier
            / per_million,
    );
    let total = components.values().sum();
    Ok(piko_protocol::messages::UsageCost {
        entries: vec![piko_protocol::messages::UsageCostEntry {
            currency: currency.into(),
            basis,
            components,
            total,
        }],
    })
}

fn schedule(plan: &BillingPlan) -> Result<StandardTokenPricing, String> {
    serde_json::from_value(plan.configuration.clone())
        .map_err(|error| format!("Invalid token pricing configuration: {error}"))
}

fn metric(usage: &BillableUsage, name: &str) -> Result<f64, String> {
    usage
        .get(name)
        .copied()
        .ok_or_else(|| format!("Token usage adapter omitted {name}"))
}
