use crate::api::{Command, ProtocolError, ServerMessage};
use crate::domain::commands::command_catalog;
use tokio::sync::mpsc::UnboundedSender;

use super::host_server::HostServer;
use super::{now_ms, send_event};

impl HostServer {
    pub(super) async fn apply_command_stream(
        &self,
        command: Command,
        command_id: String,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        match command {
            Command::AuthLoginOAuth { provider, .. } => {
                self.start_oauth_login(&command_id, provider, tx);
                Ok(())
            }
            Command::ChatSubmit {
                session_id,
                target_agent_instance_id,
                text,
                ..
            } => {
                self.0
                    .apply_chat_submit(command_id, session_id, target_agent_instance_id, text, tx)
                    .await
            }
            Command::SessionCompact { session_id, .. } => {
                // Manual compaction — bypass threshold, always compact.
                send_event(
                    tx,
                    ServerMessage::CommandResponse {
                        command_id: command_id.clone(),
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                );
                self.0
                    .compact_session_if_needed(&command_id, &session_id, 0, tx)
                    .await;
                Ok(())
            }
            command => {
                let events = self.apply_command(command).await?;
                for event in events {
                    send_event(tx, event);
                }
                Ok(())
            }
        }
    }

    async fn apply_command(&self, command: Command) -> Result<Vec<ServerMessage>, ProtocolError> {
        let command_id = command.command_id().to_string();
        if let Command::ConfigUpdate { .. } = command {
            return self.apply_config_update(&command_id, command).await;
        }

        match command {
            Command::AuthLoginOAuth { .. } => unreachable!("auth oauth handled in stream"),
            Command::ChatSubmit { .. } => {
                unreachable!("streaming chat commands handled in stream")
            }
            Command::AuthSetApiKey {
                provider, api_key, ..
            } => {
                self.apply_auth_set_api_key(&command_id, provider, api_key)
                    .await
            }
            Command::AuthLogout { provider, .. } => {
                self.apply_auth_logout(&command_id, provider).await
            }
            Command::SessionCreate { cwd, .. } => {
                self.0.apply_session_create(&command_id, cwd).await
            }
            Command::SessionOpen {
                session_id,
                session_path,
                ..
            } => {
                self.0
                    .apply_session_open(&command_id, session_id, session_path)
                    .await
            }
            Command::SessionList { scope, cwd, .. } => {
                self.0.apply_session_list(&command_id, scope, cwd).await
            }
            Command::ModelList { .. } => {
                let registry = self.model_registry.lock().await;
                let providers = registry.list_providers();
                Ok(vec![ServerMessage::CommandResponse {
                    command_id: command_id.clone(),
                    result: Ok(crate::api::CommandResult::ModelListed {
                        providers,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::CommandCatalogGet { .. } => Ok(vec![ServerMessage::CommandResponse {
                command_id: command_id.clone(),
                result: Ok(crate::api::CommandResult::CommandCatalogListed {
                    commands: command_catalog(),
                    timestamp: now_ms(),
                }),
            }]),
            Command::SessionFork {
                session_id,
                entry_id,
                ..
            } => {
                self.0
                    .apply_session_fork(&command_id, session_id, entry_id)
                    .await
            }
            Command::SessionImport { path, .. } => {
                self.0.apply_session_import(&command_id, path).await
            }
            Command::SessionRename {
                session_id, name, ..
            } => {
                self.0
                    .apply_session_rename(&command_id, session_id, name)
                    .await
            }
            Command::SessionDelete { session_id, .. } => {
                self.0.apply_session_delete(&command_id, session_id).await
            }
            Command::SessionNavigate {
                session_id,
                entry_id,
                summarize,
                custom_instructions,
                ..
            } => {
                self.0
                    .apply_session_navigate(
                        &command_id,
                        session_id,
                        entry_id,
                        summarize,
                        custom_instructions,
                    )
                    .await
            }
            Command::SessionSetLabel {
                session_id,
                entry_id,
                label,
                ..
            } => {
                self.0
                    .apply_session_set_label(&command_id, session_id, entry_id, label)
                    .await
            }
            Command::StateSnapshot { session_id, .. } => {
                self.0.apply_session_snapshot(&command_id, session_id).await
            }
            Command::QueueSteer {
                session_id,
                agent_instance_id,
                message,
                ..
            } => {
                let (queue_ev, has_active_turn) = {
                    let mut state = self.state.lock().await;
                    let queue_ev = state.push_steer(&session_id, &agent_instance_id, &message);
                    let has_active_turn = state
                        .session(&session_id)
                        .ok()
                        .and_then(|s| s.active_turn_id.clone())
                        .is_some();
                    (queue_ev, has_active_turn)
                };
                // Also route to the active root Agent if a Turn is running.
                if has_active_turn {
                    let runner = self.turn_runner.lock().await.clone();
                    let _ = runner.steer_active_agent(&session_id, &message).await;
                }
                Ok(vec![
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                    queue_ev.into(),
                ])
            }
            Command::QueueFollowUp {
                session_id,
                message,
                ..
            } => {
                let mut state = self.state.lock().await;
                let queue_ev = state.push_follow_up(&session_id, &message);
                Ok(vec![
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                    queue_ev.into(),
                ])
            }
            Command::QueueNextTurn {
                session_id,
                message,
                ..
            } => {
                let mut state = self.state.lock().await;
                let queue_ev = state.push_next_turn(&session_id, &message);
                Ok(vec![
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                    queue_ev.into(),
                ])
            }
            Command::TurnCancel {
                command_id,
                session_id,
                turn_id,
            } => {
                let runner = self.turn_runner.lock().await.clone();
                if !runner.cancel_turn_run(&session_id, &turn_id).await {
                    return Err(ProtocolError::InvalidCommand(format!(
                        "no active Agent run for Turn {turn_id}"
                    )));
                }
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::Empty),
                }])
            }
            Command::ApprovalRespond {
                command_id,
                session_id,
                approval_id,
                decision,
                ..
            } => {
                self.turn_runner
                    .lock()
                    .await
                    .clone()
                    .respond_approval(&approval_id, decision.clone())
                    .await?;
                Ok(vec![
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                    ServerMessage::Approval(crate::api::ApprovalEvent::Resolved {
                        session_id,
                        approval_id,
                        decision,
                    }),
                ])
            }
            Command::UserInteractionRespond {
                command_id,
                session_id,
                interaction_id,
                response,
                ..
            } => {
                self.turn_runner
                    .lock()
                    .await
                    .clone()
                    .respond_user_interaction(&interaction_id, response.clone())
                    .await?;
                let status = match response {
                    crate::api::UserInteractionResponse::Submit { .. } => {
                        crate::api::UserInteractionStatus::Submitted
                    }
                    crate::api::UserInteractionResponse::Cancel { .. } => {
                        crate::api::UserInteractionStatus::Cancelled
                    }
                };
                Ok(vec![
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Ok(crate::api::CommandResult::Empty),
                    },
                    ServerMessage::Interaction(piko_protocol::InteractionEvent::Resolved {
                        session_id,
                        interaction_id,
                        status,
                    }),
                ])
            }
            Command::ConfigGet { namespace, .. } => {
                let settings = self.settings.lock().await;
                let value = match namespace.as_str() {
                    "tui" => settings
                        .tui
                        .clone()
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                    _ => serde_json::Value::Object(Default::default()),
                };
                Ok(vec![ServerMessage::CommandResponse {
                    command_id: command_id.clone(),
                    result: Ok(crate::api::CommandResult::ConfigEntry { namespace, value }),
                }])
            }
            Command::ConfigUpdate { .. } => unreachable!("config_update handled before state lock"),
            Command::SessionCompact { .. } => {
                unreachable!("session_compact handled in streaming path")
            }
            Command::AgentSpecList { command_id } => {
                let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
                let agents = crate::adapters::prompts::agent_loader::load_agents(&cwd);
                let agent_list: Vec<_> = agents.values().cloned().collect();
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::AgentSpecListed {
                        agents: agent_list,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::AgentList {
                session_id,
                command_id,
            } => {
                let runner = self.turn_runner.lock().await.clone();
                let agents = if let Some(agents) = runner.list_agent_instances(&session_id).await {
                    agents
                } else {
                    self.state.lock().await.get_agent_list(&session_id)
                };
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::AgentListed {
                        session_id,
                        agents,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::AgentSubscribe {
                session_id,
                agent_instance_id,
                after_seq,
                command_id,
            } => {
                let (snapshot, replay) = {
                    let mut state = self.state.lock().await;
                    state.set_active_task(&session_id, &agent_instance_id)?;
                    let snapshot = state.agent_view_snapshot(&session_id, &agent_instance_id)?;
                    let replay =
                        state.agent_view_replay(&session_id, &agent_instance_id, after_seq)?;
                    (snapshot, replay)
                };
                if let Some(storage) = &self.storage {
                    let session_dir = self
                        .session_paths
                        .lock()
                        .await
                        .get(&session_id)
                        .cloned()
                        .ok_or_else(|| ProtocolError::SessionNotFound(session_id.clone()))?;
                    storage
                        .set_selected_agent(&session_dir, &agent_instance_id, now_ms())
                        .map_err(crate::util::storage_error)?;
                }
                let next_seq = snapshot.next_seq;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::AgentSubscribed {
                        session_id,
                        agent_instance_id,
                        agent_id: snapshot.agent_id.clone(),
                        snapshot,
                        replay,
                        next_seq,
                    }),
                }])
            }
            Command::AgentUnsubscribe {
                agent_instance_id: _,
                command_id,
                ..
            } => Ok(vec![ServerMessage::CommandResponse {
                command_id,
                result: Ok(crate::api::CommandResult::Empty),
            }]),
        }
    }
}
