mod standard;
mod time_of_day;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use piko_protocol::messages::{Usage, UsageCost};

use crate::modeling::BillingPlan;

pub use standard::{StandardTokenPricing, StandardTokenTier};
pub use time_of_day::{TimeOfDayPricing, TimeWindowPricing};

pub type BillableUsage = BTreeMap<String, f64>;

#[derive(Debug, Clone)]
pub struct BillingContext<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub api_surface: &'a str,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

pub trait UsageAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn adapt(&self, context: &BillingContext<'_>, usage: &Usage) -> Result<BillableUsage, String>;
}

pub trait PricingPolicy: Send + Sync {
    fn id(&self) -> &'static str;
    fn validate(&self, plan: &BillingPlan) -> Result<(), String>;
    fn estimate(
        &self,
        context: &BillingContext<'_>,
        plan: &BillingPlan,
        usage: &BillableUsage,
    ) -> Result<UsageCost, String>;
}

#[derive(Default)]
pub struct BillingRegistry {
    adapters: HashMap<String, Arc<dyn UsageAdapter>>,
    policies: HashMap<String, Arc<dyn PricingPolicy>>,
}

impl BillingRegistry {
    pub fn standard() -> Self {
        let mut registry = Self::default();
        registry
            .register_usage_adapter(Arc::new(standard::SemanticTokenAdapter))
            .expect("standard adapter ID is unique");
        registry
            .register_pricing_policy(Arc::new(standard::TokenTieredPolicy))
            .expect("standard policy ID is unique");
        registry
            .register_pricing_policy(Arc::new(time_of_day::TimeOfDayPolicy))
            .expect("time-of-day policy ID is unique");
        registry
    }

    pub fn register_usage_adapter(&mut self, adapter: Arc<dyn UsageAdapter>) -> Result<(), String> {
        let id = adapter.id();
        if id.is_empty() || self.adapters.contains_key(id) {
            return Err(format!("Duplicate or empty usage adapter ID: {id}"));
        }
        self.adapters.insert(id.into(), adapter);
        Ok(())
    }

    pub fn register_pricing_policy(
        &mut self,
        policy: Arc<dyn PricingPolicy>,
    ) -> Result<(), String> {
        let id = policy.id();
        if id.is_empty() || self.policies.contains_key(id) {
            return Err(format!("Duplicate or empty pricing policy ID: {id}"));
        }
        self.policies.insert(id.into(), policy);
        Ok(())
    }

    pub fn validate(&self, plan: &BillingPlan) -> Result<(), String> {
        if !self.adapters.contains_key(&plan.usage_adapter) {
            return Err(format!("Unknown usage adapter: {}", plan.usage_adapter));
        }
        self.policies
            .get(&plan.pricing_policy)
            .ok_or_else(|| format!("Unknown pricing policy: {}", plan.pricing_policy))?
            .validate(plan)
    }

    pub fn estimate(
        &self,
        context: &BillingContext<'_>,
        plan: &BillingPlan,
        usage: &Usage,
    ) -> Result<UsageCost, String> {
        let adapter = self
            .adapters
            .get(&plan.usage_adapter)
            .ok_or_else(|| format!("Unknown usage adapter: {}", plan.usage_adapter))?;
        let billable = adapter.adapt(context, usage)?;
        validate_values("billable usage", &billable)?;
        let policy = self
            .policies
            .get(&plan.pricing_policy)
            .ok_or_else(|| format!("Unknown pricing policy: {}", plan.pricing_policy))?;
        let cost = policy.estimate(context, plan, &billable)?;
        for entry in &cost.entries {
            validate_values("cost components", &entry.components)?;
            let total: f64 = entry.components.values().sum();
            if !entry.total.is_finite() || (entry.total - total).abs() > 1e-9 {
                return Err("Pricing policy emitted an inconsistent total".into());
            }
        }
        Ok(cost)
    }
}

fn validate_values(label: &str, values: &BTreeMap<String, f64>) -> Result<(), String> {
    if values
        .iter()
        .any(|(name, value)| name.is_empty() || !value.is_finite() || *value < 0.0)
    {
        Err(format!("{label} contains an invalid name or value"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod tests_support {
    use piko_protocol::messages::Usage;

    use super::standard::StandardTokenPricing;

    pub(crate) fn schedule(rates: [f64; 3]) -> StandardTokenPricing {
        StandardTokenPricing {
            input_per_million: rates[0],
            cached_input_per_million: rates[1],
            output_per_million: rates[2],
            cache_write_per_million: None,
            tiers: Vec::new(),
        }
    }

    pub(crate) fn usage(input: u64, output: u64, read: u64, write: u64) -> Usage {
        Usage {
            input,
            output,
            cache_read: read,
            cache_write: write,
            total_tokens: input + output,
            ..Default::default()
        }
    }
}
