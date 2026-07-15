use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::api::{AgentRunEvent, CommandResult, ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::ports::{AgentInputRunHandle, AgentInputRunInput, AgentOperationAddress, TurnRunner};
use crate::util::{now_ms, send_event, storage_error};

impl HostApp {
    pub(crate) async fn submit_direct_agent_chat(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
        text: String,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let cwd = self.state.lock().await.session_cwd(&session_id)?;
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        let store = self.session_store_factory.open(&session_dir);
        let manifest = store.load_manifest().map_err(storage_error)?;
        if manifest.root_agent_instance_id.as_deref() == Some(agent_instance_id.as_str()) {
            return Err(ProtocolError::InvalidCommand(
                "direct agent chat cannot target the session root".into(),
            ));
        }
        if !manifest.agents.contains_key(&agent_instance_id) {
            return Err(ProtocolError::InvalidCommand(format!(
                "agent instance not found: {agent_instance_id}"
            )));
        }

        let run_id = format!("agent_run_{}", uuid::Uuid::new_v4());
        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let (ui_event_tx, ui_event_rx) = unbounded_channel();
        let runner = self.turn_runner.lock().await.clone();
        let run = runner
            .run_agent_input(AgentInputRunInput {
                session_id: session_id.clone(),
                run_id: run_id.clone(),
                agent_instance_id: agent_instance_id.clone(),
                prompt: text,
                cwd,
                active_tool_names,
                session_dir: session_dir.clone(),
                ui_event_tx,
            })
            .await?;

        send_event(
            tx,
            ServerMessage::CommandResponse {
                command_id,
                result: Ok(CommandResult::Empty),
            },
        );
        send_event(
            tx,
            ServerMessage::AgentRunLifecycle(AgentRunEvent::Started {
                session_id: session_id.clone(),
                run_id: run_id.clone(),
                agent_instance_id: agent_instance_id.clone(),
                timestamp: now_ms(),
            }),
        );
        let observed = self
            .run_agent_input_observation_loop(
                &runner,
                &session_id,
                &run_id,
                &agent_instance_id,
                &session_dir,
                run,
                ui_event_rx,
                tx,
            )
            .await;
        if let Err(error) = observed {
            send_event(
                tx,
                ServerMessage::AgentRunLifecycle(AgentRunEvent::Failed {
                    session_id: session_id.clone(),
                    run_id: run_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    error: error.to_string(),
                    timestamp: now_ms(),
                }),
            );
            runner
                .finish_agent_input(&session_id, &agent_instance_id, &run_id)
                .await;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_agent_input_observation_loop(
        &self,
        runner: &Arc<dyn TurnRunner>,
        session_id: &str,
        run_id: &str,
        agent_instance_id: &str,
        session_dir: &std::path::Path,
        run: AgentInputRunHandle,
        ui_event_rx: tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let address = AgentOperationAddress {
            session_id: session_id.to_string(),
            operation_id: run_id.to_string(),
            agent_instance_id: agent_instance_id.to_string(),
        };
        let completed = self
            .drive_operation_observation(
                runner,
                &address,
                session_dir,
                run.observation,
                run.completion,
                ui_event_rx,
                tx,
            )
            .await?;
        let lifecycle = match completed.result {
            Ok(report)
                if matches!(
                    report.outcome,
                    piko_protocol::ExecutionOutcome::Succeeded { .. }
                ) =>
            {
                AgentRunEvent::Completed {
                    session_id: session_id.to_string(),
                    run_id: run_id.to_string(),
                    agent_instance_id: agent_instance_id.to_string(),
                    timestamp: now_ms(),
                }
            }
            Ok(report) => AgentRunEvent::Failed {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                error: format!("agent run ended with {:?}", report.outcome),
                timestamp: now_ms(),
            },
            Err(failure) => AgentRunEvent::Failed {
                session_id: session_id.to_string(),
                run_id: run_id.to_string(),
                agent_instance_id: agent_instance_id.to_string(),
                error: failure.message,
                timestamp: now_ms(),
            },
        };
        send_event(tx, ServerMessage::AgentRunLifecycle(lifecycle));
        runner
            .finish_agent_input(session_id, agent_instance_id, run_id)
            .await;
        Ok(())
    }
}
