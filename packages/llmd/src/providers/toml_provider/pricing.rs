use std::collections::HashMap;

use piko_protocol::messages::UsageCostBasis;
use serde::Deserialize;

use crate::modeling::{ApiSurface, TokenPricing, TokenPricingTier};

#[derive(Debug, Clone, Deserialize)]
pub(super) struct PricingToml {
    surface: String,
    basis: String,
    #[serde(default)]
    copy_from: Option<String>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    input_per_million: Option<f64>,
    #[serde(default)]
    cached_input_per_million: Option<f64>,
    #[serde(default)]
    output_per_million: Option<f64>,
    #[serde(default)]
    cache_write_per_million: Option<f64>,
    #[serde(default)]
    tiers: Vec<PricingTierToml>,
}

#[derive(Debug, Clone, Deserialize)]
struct PricingTierToml {
    input_tokens_above: u64,
    input_multiplier: f64,
    output_multiplier: f64,
}

fn basis(value: &str) -> Result<UsageCostBasis, String> {
    match value {
        "list_price" => Ok(UsageCostBasis::ListPrice),
        "api_equivalent" => Ok(UsageCostBasis::ApiEquivalent),
        _ => Err(format!("Unknown pricing basis: {value}")),
    }
}

fn validate_currency(currency: &str) -> Result<(), String> {
    if currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(format!(
            "Pricing currency must be an ISO-style uppercase code: {currency}"
        ))
    }
}

fn direct(spec: &PricingToml) -> Result<TokenPricing, String> {
    if spec.copy_from.is_some() {
        return Err("Copied pricing cannot be parsed as a direct schedule".into());
    }
    let currency = spec
        .currency
        .as_deref()
        .ok_or_else(|| format!("Pricing for {} has no currency", spec.surface))?;
    validate_currency(currency)?;
    let positive = |name: &str, value: Option<f64>| {
        value
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| format!("Pricing for {} has invalid {name}", spec.surface))
    };
    let tiers = spec
        .tiers
        .iter()
        .map(|tier| {
            if tier.input_multiplier <= 0.0 || tier.output_multiplier <= 0.0 {
                return Err(format!(
                    "Pricing for {} has a non-positive tier multiplier",
                    spec.surface
                ));
            }
            Ok(TokenPricingTier {
                input_tokens_above: tier.input_tokens_above,
                input_multiplier: tier.input_multiplier,
                output_multiplier: tier.output_multiplier,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(TokenPricing {
        currency: currency.into(),
        basis: basis(&spec.basis)?,
        input_per_million: positive("input_per_million", spec.input_per_million)?,
        cached_input_per_million: positive(
            "cached_input_per_million",
            spec.cached_input_per_million,
        )?,
        output_per_million: positive("output_per_million", spec.output_per_million)?,
        cache_write_per_million: spec
            .cache_write_per_million
            .map(|value| positive("cache_write_per_million", Some(value)))
            .transpose()?,
        tiers,
    })
}

pub(super) fn build_pricing(
    specs_by_model: HashMap<String, Vec<PricingToml>>,
    surfaces: &HashMap<String, ApiSurface>,
) -> Result<HashMap<String, HashMap<String, TokenPricing>>, String> {
    let mut result = HashMap::new();
    for (model_id, specs) in specs_by_model {
        let mut schedules = HashMap::new();
        for spec in specs.iter().filter(|spec| spec.copy_from.is_none()) {
            if !surfaces.contains_key(&spec.surface) {
                return Err(format!(
                    "Pricing for {model_id} references unknown surface {}",
                    spec.surface
                ));
            }
            if schedules
                .insert(spec.surface.clone(), direct(spec)?)
                .is_some()
            {
                return Err(format!("Duplicate pricing for {model_id}/{}", spec.surface));
            }
        }
        for spec in specs.iter().filter(|spec| spec.copy_from.is_some()) {
            if !surfaces.contains_key(&spec.surface) {
                return Err(format!(
                    "Pricing for {model_id} references unknown surface {}",
                    spec.surface
                ));
            }
            let source = spec.copy_from.as_deref().unwrap();
            let mut schedule = schedules.get(source).cloned().ok_or_else(|| {
                format!(
                    "Pricing for {model_id}/{} copies missing surface {source}",
                    spec.surface
                )
            })?;
            schedule.basis = basis(&spec.basis)?;
            if let Some(currency) = &spec.currency {
                validate_currency(currency)?;
                schedule.currency.clone_from(currency);
            }
            if schedules.insert(spec.surface.clone(), schedule).is_some() {
                return Err(format!("Duplicate pricing for {model_id}/{}", spec.surface));
            }
        }
        if !schedules.is_empty() {
            result.insert(model_id, schedules);
        }
    }
    Ok(result)
}
