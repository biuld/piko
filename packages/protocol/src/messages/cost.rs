use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageCost {
    pub entries: Vec<UsageCostEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageCostEntry {
    pub currency: String,
    pub basis: UsageCostBasis,
    pub components: BTreeMap<String, f64>,
    pub total: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageCostBasis {
    ListPrice,
    ApiEquivalent,
}

impl UsageCostBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ListPrice => "list_price",
            Self::ApiEquivalent => "api_equivalent",
        }
    }
}

impl UsageCost {
    pub fn accumulate(&mut self, other: &Self) {
        for incoming in &other.entries {
            if let Some(existing) = self
                .entries
                .iter_mut()
                .find(|entry| entry.currency == incoming.currency && entry.basis == incoming.basis)
            {
                for (name, amount) in &incoming.components {
                    *existing.components.entry(name.clone()).or_default() += amount;
                }
                existing.total = existing.components.values().sum();
            } else {
                self.entries.push(incoming.clone());
            }
        }
    }
}
