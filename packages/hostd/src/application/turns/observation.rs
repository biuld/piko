use tokio::sync::mpsc::UnboundedSender;

use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::util::send_event;

impl HostApp {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_execution_observation(
        &self,
        runner: &std::sync::Arc<dyn crate::ports::TurnRunner>,
        cwd: &str,
        session_id: &str,
        agent_specs: &std::collections::HashMap<String, piko_protocol::agents::AgentSpec>,
        snapshot: &piko_protocol::ExecutionObservationSnapshot,
        total_tasks: &mut u32,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let execution_id = &snapshot.execution_id;
        let agent_instance_id = &snapshot.agent_instance_id;
        let agent_id = &snapshot.agent_id;
        let agent_status = match snapshot.status {
            piko_protocol::ExecutionStatus::Accepted | piko_protocol::ExecutionStatus::Running => {
                crate::api::AgentStatus::Running
            }
            piko_protocol::ExecutionStatus::Succeeded => crate::api::AgentStatus::Idle,
            piko_protocol::ExecutionStatus::Failed => crate::api::AgentStatus::Failed,
            piko_protocol::ExecutionStatus::Cancelled => crate::api::AgentStatus::Cancelled,
        };

        let mut state = self.state.lock().await;
        let (info, created) = {
            let session = state.session_mut(session_id)?;
            let created = !session.active_agents.contains_key(agent_instance_id);
            if created {
                *total_tasks += 1;
                session.active_agent_instance_id = Some(agent_instance_id.clone());
            }
            let entry = session
                .active_agents
                .entry(agent_instance_id.clone())
                .or_insert_with(|| crate::api::AgentInfo {
                    agent_instance_id: agent_instance_id.clone(),
                    agent_id: agent_id.clone(),
                    parent_agent_instance_id: None,
                    lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                    activity: if agent_status == crate::api::AgentStatus::Running {
                        piko_protocol::AgentActivity::Running {
                            execution_id: execution_id.clone(),
                        }
                    } else {
                        piko_protocol::AgentActivity::Idle
                    },
                    unread_report_count: 0,
                    name: agent_specs
                        .get(agent_id)
                        .map(|spec| spec.name.clone())
                        .unwrap_or_else(|| agent_id.clone()),
                    role: agent_specs
                        .get(agent_id)
                        .map(|spec| spec.role.clone())
                        .unwrap_or_else(|| "assistant".into()),
                    status: agent_status.clone(),
                });
            entry.status = agent_status;
            entry.activity = if entry.status == crate::api::AgentStatus::Running {
                piko_protocol::AgentActivity::Running {
                    execution_id: execution_id.clone(),
                }
            } else {
                piko_protocol::AgentActivity::Idle
            };
            (entry.clone(), created)
        };
        let changed = ServerMessage::AgentChanged(info);
        let _ = state.append_agent_view_event(
            session_id,
            agent_instance_id,
            agent_id,
            changed.clone(),
        )?;
        drop(state);

        if created {
            runner.on_task_created(execution_id, session_id, cwd).await;
        }

        send_event(tx, changed);
        Ok(())
    }
}
