use std::collections::HashMap;

use crate::api::{AgentInfo, ProtocolError, ServerMessage, SessionSnapshot};
use crate::application::host_app::HostApp;
use crate::util::now_ms;

pub(super) fn server_response_ok(
    command_id: &str,
    result: crate::api::CommandResult,
) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

pub(crate) fn session_reconciled_message(
    session_id: String,
    reason: piko_protocol::ReconcileReason,
    snapshot: crate::api::SessionSnapshot,
    agents: Vec<crate::api::AgentInfo>,
) -> ServerMessage {
    let cursor = piko_protocol::agent_runtime::SessionCursor {
        epoch: format!("hostd:{session_id}"),
        seq: snapshot.seq,
    };
    ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
        session_id,
        reason,
        cursor,
        snapshot,
        agents,
    })
}

pub(super) fn session_opened_messages(
    command_id: &str,
    session_id: String,
    snapshot: crate::api::SessionSnapshot,
    agents: Vec<crate::api::AgentInfo>,
    interrupt_events: Vec<ServerMessage>,
) -> Vec<ServerMessage> {
    let mut messages = interrupt_events;
    messages.push(server_response_ok(
        command_id,
        crate::api::CommandResult::SessionOpened {
            session_id: session_id.clone(),
            timestamp: now_ms(),
        },
    ));
    messages.push(session_reconciled_message(
        session_id,
        piko_protocol::ReconcileReason::InitialHydration,
        snapshot,
        agents,
    ));
    messages
}

impl HostApp {
    /// Enrich a domain snapshot with in-process pending prompts / live agents.
    pub(crate) async fn enrich_session_view(
        &self,
        session_id: &str,
        mut snapshot: SessionSnapshot,
        mut agents: Vec<AgentInfo>,
    ) -> (SessionSnapshot, Vec<AgentInfo>) {
        let runner = self.turn_runner.lock().await.clone();
        if let Some(live_agents) = runner.list_agent_instances(session_id).await {
            agents = live_agents;
        }
        merge_agent_usage_runtime(
            &mut snapshot,
            &agents,
            self.session_paths.lock().await.get(session_id).cloned(),
            &self.session_store_factory,
        )
        .await;
        let (approvals, interactions) = runner.pending_prompts_for_session(session_id).await;
        snapshot.pending_approvals = approvals;
        snapshot.pending_interactions = interactions;
        for turn in &mut snapshot.active_turns {
            if snapshot
                .pending_approvals
                .iter()
                .any(|approval| approval.agent_instance_id == turn.agent_instance_id)
                || snapshot
                    .pending_interactions
                    .iter()
                    .any(|interaction| interaction.agent_instance_id == turn.agent_instance_id)
            {
                turn.status = crate::api::TurnStatus::WaitingForApproval;
            } else if agents.iter().any(|agent| {
                agent.agent_instance_id == turn.agent_instance_id
                    && agent.activity == piko_protocol::AgentActivity::Cancelling
            }) {
                turn.status = crate::api::TurnStatus::Cancelling;
            }
        }
        (snapshot, agents)
    }

    pub(super) async fn enrich_reconcile_messages(
        &self,
        session_id: &str,
        mut messages: Vec<ServerMessage>,
    ) -> Vec<ServerMessage> {
        // F-27: seed orchd runtime todo store from host durable lists on hydrate.
        self.seed_orch_todo_lists(session_id).await;
        for message in &mut messages {
            if let ServerMessage::SessionReconciled(reconciled) = message {
                let (snapshot, agents) = self
                    .enrich_session_view(
                        session_id,
                        reconciled.snapshot.clone(),
                        reconciled.agents.clone(),
                    )
                    .await;
                reconciled.snapshot = snapshot;
                reconciled.agents = agents;
                reconciled.cursor.seq = reconciled.snapshot.seq;
            }
        }
        messages
    }

    /// Push host-durable todo lists into the orch runtime tool provider.
    pub(crate) async fn seed_orch_todo_lists(&self, session_id: &str) {
        let lists = {
            let state = self.state.lock().await;
            state
                .session(session_id)
                .map(|s| s.todo_lists_for_snapshot())
                .unwrap_or_default()
        };
        let runner = self.turn_runner.lock().await.clone();
        runner.seed_todo_lists(lists).await;
    }

    pub(crate) async fn session_view(
        &self,
        session_id: &str,
    ) -> Result<(SessionSnapshot, Vec<AgentInfo>), ProtocolError> {
        let (snapshot, agents) = {
            let state = self.state.lock().await;
            (
                state.snapshot(session_id)?,
                state.get_agent_list(session_id),
            )
        };
        Ok(self.enrich_session_view(session_id, snapshot, agents).await)
    }
}

async fn merge_agent_usage_runtime(
    snapshot: &mut SessionSnapshot,
    agents: &[AgentInfo],
    session_dir: Option<std::path::PathBuf>,
    store_factory: &std::sync::Arc<dyn crate::ports::SessionStoreFactory>,
) {
    let mut row_by_instance = snapshot
        .agent_usage
        .drain(..)
        .map(|row| (row.agent_instance_id.clone(), row))
        .collect::<HashMap<_, _>>();

    for agent in agents {
        row_by_instance
            .entry(agent.agent_instance_id.clone())
            .or_insert_with(|| piko_protocol::AgentUsageSummary {
                agent_instance_id: agent.agent_instance_id.clone(),
                agent_id: agent.agent_id.clone(),
                run_count: None,
                active_duration_ms: None,
                usage: piko_protocol::Usage::empty(),
            });
    }

    if let Some(session_dir) = session_dir
        && let Ok(projection) = store_factory.open(&session_dir).load_projection().await
    {
        for row in row_by_instance.values_mut() {
            row.run_count = Some(0);
            row.active_duration_ms = Some(0);
        }
        merge_execution_stats(
            &mut row_by_instance,
            agents,
            projection.agent_executions.into_values(),
            now_ms(),
        );
    }

    let order = agents
        .iter()
        .enumerate()
        .map(|(index, agent)| (agent.agent_instance_id.as_str(), index))
        .collect::<HashMap<_, _>>();
    snapshot.agent_usage = row_by_instance.into_values().collect();
    snapshot.agent_usage.sort_by(|left, right| {
        order
            .get(left.agent_instance_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &order
                    .get(right.agent_instance_id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| left.agent_instance_id.cmp(&right.agent_instance_id))
    });
}

fn merge_execution_stats(
    rows: &mut HashMap<String, piko_protocol::AgentUsageSummary>,
    agents: &[AgentInfo],
    executions: impl IntoIterator<Item = crate::ports::storage_types::ExecutionProjection>,
    snapshot_at: i64,
) {
    for execution in executions {
        let row = rows
            .entry(execution.agent_instance_id.clone())
            .or_insert_with(|| piko_protocol::AgentUsageSummary {
                agent_instance_id: execution.agent_instance_id.clone(),
                agent_id: agents
                    .iter()
                    .find(|agent| agent.agent_instance_id == execution.agent_instance_id)
                    .map(|agent| agent.agent_id.clone())
                    .unwrap_or_else(|| execution.agent_instance_id.clone()),
                run_count: Some(0),
                active_duration_ms: Some(0),
                usage: piko_protocol::Usage::empty(),
            });
        row.run_count = Some(row.run_count.unwrap_or_default().saturating_add(1));
        let finished_at = execution.finished_at.unwrap_or(snapshot_at);
        let duration_ms = finished_at.saturating_sub(execution.started_at).max(0) as u64;
        row.active_duration_ms = Some(
            row.active_duration_ms
                .unwrap_or_default()
                .saturating_add(duration_ms),
        );
    }
}

#[cfg(test)]
mod usage_tests {
    use super::merge_execution_stats;
    use crate::ports::storage_types::ExecutionProjection;
    use std::collections::HashMap;

    fn execution(id: &str, started_at: i64, finished_at: Option<i64>) -> ExecutionProjection {
        ExecutionProjection {
            agent_instance_id: id.into(),
            run_id: format!("run-{started_at}"),
            execution_id: format!("execution-{started_at}"),
            request_id: String::new(),
            source_turn_id: None,
            detached_recipient_agent_instance_id: None,
            detached_report_delivered: false,
            prompt_assembly_version: 0,
            prompt_digest: String::new(),
            status: if finished_at.is_some() {
                piko_protocol::ExecutionStatus::Succeeded
            } else {
                piko_protocol::ExecutionStatus::Running
            },
            started_at,
            finished_at,
            report: None,
        }
    }

    #[test]
    fn execution_stats_count_runs_and_include_running_elapsed_time() {
        let mut rows = HashMap::new();
        merge_execution_stats(
            &mut rows,
            &[],
            [
                execution("agent-1", 100, Some(350)),
                execution("agent-1", 500, None),
            ],
            1_000,
        );

        let row = &rows["agent-1"];
        assert_eq!(row.run_count, Some(2));
        assert_eq!(row.active_duration_ms, Some(750));
    }
}
