use std::collections::{BTreeMap, BTreeSet};

use piko_protocol::Usage;
use serde::{Deserialize, Serialize};

use crate::{Result, StoreError, UsageCorrectedV1, UsageRecordedV1};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffectiveUsageFact {
    pub recorded: UsageRecordedV1,
    pub effective_usage: Usage,
    pub correction_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AccountingProjection {
    facts: BTreeMap<String, EffectiveUsageFact>,
    correction_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageSummary {
    pub usage: Usage,
    pub fact_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageQuery {
    pub agent_instance_id: Option<String>,
    pub root_input_id: Option<String>,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub incurred_only: bool,
}

impl AccountingProjection {
    pub fn fact(&self, usage_id: &str) -> Option<&EffectiveUsageFact> {
        self.facts.get(usage_id)
    }

    pub fn summarize_incurred(&self) -> UsageSummary {
        self.summarize(&UsageQuery {
            incurred_only: true,
            ..UsageQuery::default()
        })
    }

    pub fn summarize(&self, query: &UsageQuery) -> UsageSummary {
        let mut summary = UsageSummary::default();
        for fact in self.facts(query) {
            summary.usage.accumulate(&fact.effective_usage);
            summary.fact_count += 1;
        }
        summary
    }

    pub fn facts<'a>(
        &'a self,
        query: &'a UsageQuery,
    ) -> impl Iterator<Item = &'a EffectiveUsageFact> + 'a {
        self.facts.values().filter(move |fact| {
            let attribution = &fact.recorded.attribution;
            (!query.incurred_only || fact.recorded.incurred)
                && query
                    .agent_instance_id
                    .as_ref()
                    .is_none_or(|id| id == &attribution.agent_instance_id)
                && query
                    .root_input_id
                    .as_ref()
                    .is_none_or(|id| id == &attribution.root_input_id)
                && query
                    .provider
                    .as_ref()
                    .is_none_or(|provider| provider == &fact.recorded.provider)
                && query
                    .model_id
                    .as_ref()
                    .is_none_or(|model| model == &fact.recorded.model_id)
        })
    }

    pub(crate) fn record(&mut self, fact: UsageRecordedV1) -> Result<()> {
        if self.facts.contains_key(&fact.usage_id) {
            return Err(StoreError::IdempotencyConflict(fact.usage_id));
        }
        self.facts.insert(
            fact.usage_id.clone(),
            EffectiveUsageFact {
                effective_usage: fact.usage.clone(),
                recorded: fact,
                correction_ids: Vec::new(),
            },
        );
        Ok(())
    }

    pub(crate) fn correct(&mut self, correction: UsageCorrectedV1) -> Result<()> {
        if !self.correction_ids.insert(correction.correction_id.clone()) {
            return Err(StoreError::IdempotencyConflict(correction.correction_id));
        }
        let fact = self.facts.get_mut(&correction.usage_id).ok_or_else(|| {
            StoreError::InvalidEvent(format!("unknown usage fact {}", correction.usage_id))
        })?;
        fact.effective_usage = correction.replacement;
        fact.correction_ids.push(correction.correction_id);
        Ok(())
    }
}
