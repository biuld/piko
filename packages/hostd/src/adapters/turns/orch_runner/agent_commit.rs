use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc::UnboundedSender;

use orchd_api::{AgentCommitPort, AgentRecoveryState};
use piko_protocol::{AgentCommitAck, AgentDurableCommand, AgentInstanceLifecycle, CommitError};

use crate::api::ServerMessage;

pub(super) struct ProjectingAgentCommitPort {
    inner: Arc<dyn AgentCommitPort>,
    agents: std::sync::Mutex<HashMap<String, crate::api::AgentInfo>>,
    event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
}

impl ProjectingAgentCommitPort {
    pub(super) fn new(
        inner: Arc<dyn AgentCommitPort>,
        recovered: &[AgentRecoveryState],
        event_tx: Arc<std::sync::Mutex<Option<UnboundedSender<ServerMessage>>>>,
    ) -> Self {
        let agents = recovered
            .iter()
            .map(|state| {
                let status = lifecycle_status(state.lifecycle);
                (
                    state.identity.agent_instance_id.clone(),
                    crate::api::AgentInfo {
                        agent_instance_id: state.identity.agent_instance_id.clone(),
                        agent_id: state.identity.agent_spec_id.clone(),
                        parent_agent_instance_id: state.identity.parent_agent_instance_id.clone(),
                        lifecycle: state.lifecycle,
                        activity: piko_protocol::AgentActivity::Idle,
                        unread_report_count: state
                            .inbox
                            .iter()
                            .filter(|item| item.consumed_at.is_none())
                            .count() as u32,
                        name: state.spec.name.clone(),
                        role: state.spec.role.clone(),
                        status,
                    },
                )
            })
            .collect();
        Self {
            inner,
            agents: std::sync::Mutex::new(agents),
            event_tx,
        }
    }

    fn project(&self, command: AgentDurableCommand) {
        let changed = {
            let mut agents = self.agents.lock().unwrap();
            match command {
                AgentDurableCommand::Create { identity, spec } => {
                    let info = crate::api::AgentInfo {
                        agent_instance_id: identity.agent_instance_id.clone(),
                        agent_id: identity.agent_spec_id,
                        parent_agent_instance_id: identity.parent_agent_instance_id.clone(),
                        lifecycle: AgentInstanceLifecycle::Open,
                        activity: piko_protocol::AgentActivity::Idle,
                        unread_report_count: 0,
                        name: spec.name,
                        role: spec.role,
                        status: crate::api::AgentStatus::Idle,
                    };
                    agents.insert(identity.agent_instance_id, info.clone());
                    Some(info)
                }
                AgentDurableCommand::RunStarted {
                    agent_instance_id,
                    internal_execution_id: _,
                    ..
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.activity = piko_protocol::AgentActivity::Running;
                    info.status = crate::api::AgentStatus::Running;
                    info.clone()
                }),
                AgentDurableCommand::RunTerminal { report, .. } => {
                    agents.get_mut(&report.agent_instance_id).map(|info| {
                        info.activity = piko_protocol::AgentActivity::Idle;
                        info.status = crate::api::AgentStatus::Idle;
                        info.clone()
                    })
                }
                AgentDurableCommand::InputQueued { .. }
                | AgentDurableCommand::QueuedInputStarted { .. } => None,
                AgentDurableCommand::SetLifecycle {
                    agent_instance_id,
                    lifecycle,
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.lifecycle = lifecycle;
                    info.status = lifecycle_status(lifecycle);
                    info.clone()
                }),
                AgentDurableCommand::CommitReport {
                    recipient_agent_instance_id,
                    ..
                } => agents.get_mut(&recipient_agent_instance_id).map(|info| {
                    info.unread_report_count = info.unread_report_count.saturating_add(1);
                    info.clone()
                }),
                AgentDurableCommand::ConsumeInboxItem {
                    agent_instance_id, ..
                } => agents.get_mut(&agent_instance_id).map(|info| {
                    info.unread_report_count = info.unread_report_count.saturating_sub(1);
                    info.clone()
                }),
            }
        };
        if let Some(changed) = changed
            && let Some(tx) = self.event_tx.lock().unwrap().as_ref()
        {
            let _ = tx.send(ServerMessage::AgentChanged(changed));
        }
    }
}

#[async_trait]
impl AgentCommitPort for ProjectingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let ack = self
            .inner
            .commit_agent_command(session_id, command.clone())
            .await?;
        self.project(command);
        Ok(ack)
    }
}

fn lifecycle_status(lifecycle: AgentInstanceLifecycle) -> crate::api::AgentStatus {
    match lifecycle {
        AgentInstanceLifecycle::Open => crate::api::AgentStatus::Idle,
        AgentInstanceLifecycle::Closed => crate::api::AgentStatus::Closed,
        AgentInstanceLifecycle::Terminated => crate::api::AgentStatus::Stopped,
        AgentInstanceLifecycle::Unavailable => crate::api::AgentStatus::Failed,
    }
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct EphemeralAgentCommitPort {
    revision: AtomicU64,
}

#[cfg(test)]
#[async_trait]
impl AgentCommitPort for EphemeralAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        let agent_instance_id = match command {
            AgentDurableCommand::Create { identity, .. } => identity.agent_instance_id,
            AgentDurableCommand::SetLifecycle {
                agent_instance_id, ..
            }
            | AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentDurableCommand::RunTerminal { report, .. } => report.agent_instance_id,
            AgentDurableCommand::RunStarted {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentDurableCommand::InputQueued {
                agent_instance_id, ..
            }
            | AgentDurableCommand::QueuedInputStarted {
                agent_instance_id, ..
            } => agent_instance_id,
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                ..
            } => recipient_agent_instance_id,
        };
        Ok(AgentCommitAck {
            session_id: session_id.into(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}
