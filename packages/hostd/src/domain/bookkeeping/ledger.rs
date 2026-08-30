//! Incurred session / turn / agent usage ledger (F-15 / F-30 / F-32).

use crate::domain::sessions::SessionState;
use piko_protocol::messages::Usage;

impl SessionState {
    /// Accumulate usage from an assistant message (session roll-up).
    pub fn accumulate_usage(&mut self, usage: &Usage) {
        self.cumulative_usage.accumulate(usage);
    }

    /// Account one model-step usage into the product ledger.
    ///
    /// Always updates session `cumulative_usage`. When an AgentInstance is
    /// supplied, also rolls the step into that agent's incurred usage.
    pub fn account_step_usage(&mut self, agent_instance_id: Option<&str>, usage: &Usage) {
        self.accumulate_usage(usage);
        let Some(agent_instance_id) = agent_instance_id else {
            return;
        };
        self.agent_usage
            .entry(agent_instance_id.to_string())
            .or_default()
            .accumulate(usage);
    }

    /// Rebuild per-AgentInstance token/cost buckets from durable message facts.
    pub fn agent_usage_for_snapshot(&self) -> Vec<piko_protocol::AgentUsageSummary> {
        let mut rows =
            std::collections::BTreeMap::<String, piko_protocol::AgentUsageSummary>::new();

        for agent in self.active_agents.values() {
            rows.entry(agent.agent_instance_id.clone())
                .or_insert_with(|| piko_protocol::AgentUsageSummary {
                    agent_instance_id: agent.agent_instance_id.clone(),
                    agent_id: agent.agent_id.clone(),
                    run_count: None,
                    active_duration_ms: None,
                    usage: Usage::empty(),
                });
        }

        for (agent_instance_id, usage) in &self.agent_usage {
            let row = rows.entry(agent_instance_id.clone()).or_insert_with(|| {
                piko_protocol::AgentUsageSummary {
                    agent_instance_id: agent_instance_id.clone(),
                    agent_id: self
                        .active_agents
                        .get(agent_instance_id)
                        .map(|agent| agent.agent_id.clone())
                        .unwrap_or_else(|| agent_instance_id.clone()),
                    run_count: None,
                    active_duration_ms: None,
                    usage: Usage::empty(),
                }
            });
            row.usage = usage.clone();
        }

        rows.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::sessions::{HostState, SessionState};

    fn usage(input: u64, output: u64) -> Usage {
        Usage {
            input,
            output,
            total_tokens: input + output,
            ..Usage::empty()
        }
    }

    #[test]
    fn account_step_usage_rolls_session_turn_and_agent() {
        let mut state = HostState::new();
        let session_id = match state.create_session("/tmp") {
            crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
            other => panic!("unexpected create result: {other:?}"),
        };
        let session = state.session_mut(&session_id).unwrap();
        session.account_step_usage(Some("root"), &usage(10, 4));
        session.account_step_usage(Some("root"), &usage(3, 1));

        assert_eq!(session.cumulative_usage.input, 13);
        assert_eq!(session.cumulative_usage.output, 5);
        assert_eq!(session.agent_usage["root"].input, 13);
    }

    #[test]
    fn account_step_usage_without_turn_only_updates_session() {
        let mut session = SessionState::new("session_1".into(), "/tmp".into());
        session.account_step_usage(None, &usage(2, 0));
        assert_eq!(session.cumulative_usage.input, 2);
        assert!(session.agent_usage.is_empty());
    }
}
