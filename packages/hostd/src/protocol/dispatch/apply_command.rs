use super::*;

impl HostServer {
    pub(super) async fn apply_command(
        &self,
        command: Command,
    ) -> Result<Vec<ServerMessage>, ProtocolError> {
        let command_id = command.command_id().to_string();
        if let Command::ConfigUpdate { .. } = command {
            return self.apply_config_update(&command_id, command).await;
        }

        match command {
            Command::AuthLoginOAuth { .. } => unreachable!("auth oauth handled in stream"),
            Command::AuthCancelOAuth { provider, .. } => {
                self.apply_auth_cancel_oauth(&command_id, provider).await
            }
            Command::AgentInputSubmit { .. } => {
                unreachable!("streaming agent input commands handled in stream")
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
            Command::RolloutPageGet {
                session_id,
                agent_instance_id,
                after_cursor,
                limit,
                ..
            } => {
                let page = self
                    .0
                    .rollout_page(
                        &session_id,
                        &agent_instance_id,
                        after_cursor.as_deref(),
                        limit,
                    )
                    .await?;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::RolloutPaged {
                        page,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::TurnDiffGet {
                session_id,
                turn_id,
                ..
            } => {
                let diff = self.0.turn_diff(&session_id, &turn_id).await?;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::TurnDiffGot {
                        diff,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::ProcessList { .. } => {
                let runner = self.0.turn_runner.lock().await.clone();
                let processes = runner.list_processes().await;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::ProcessListed {
                        processes,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::ProcessStop { process_id, .. } => {
                let runner = self.0.turn_runner.lock().await.clone();
                let exit = runner.terminate_process(&process_id).await;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::ProcessStopped {
                        process_id,
                        stopped: exit.is_some(),
                        exit_code: exit.and_then(|e| e.exit_code),
                        signal: exit.and_then(|e| e.signal),
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::McpStatus { .. } => {
                let runner = self.0.turn_runner.lock().await.clone();
                let servers = runner.mcp_statuses().await;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::McpStatusListed {
                        servers,
                        timestamp: now_ms(),
                    }),
                }])
            }
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
            Command::AgentInputCancel {
                session_id,
                agent_instance_id,
                input_id,
                ..
            } => {
                crate::application::AgentWorkControl::new(&self.0)
                    .cancel_input(command_id, session_id, agent_instance_id, input_id)
                    .await
            }
            Command::AgentInterrupt {
                session_id,
                agent_instance_id,
                ..
            } => {
                crate::application::AgentWorkControl::new(&self.0)
                    .interrupt_current(command_id, session_id, agent_instance_id)
                    .await
            }
            Command::ApprovalRespond {
                command_id,
                session_id,
                approval_id,
                decision,
                ..
            } => {
                let handled = self
                    .turn_runner
                    .lock()
                    .await
                    .clone()
                    .respond_approval(&approval_id, decision.clone())
                    .await?;
                // Only an actually-resolved approval publishes a resolution
                // event. Late or duplicate responses after a deadline expiry
                // (or after the entry was removed) are ignored: no second
                // resolution event, no grant, no effect on the turn.
                let mut events = vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::Empty),
                }];
                if handled {
                    events.push(ServerMessage::Approval(
                        crate::api::ApprovalEvent::Resolved {
                            session_id,
                            approval_id,
                            decision,
                        },
                    ));
                }
                Ok(events)
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
                let value = settings.namespace_value(&namespace);
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
                        .await
                        .map_err(crate::util::storage_error)?;
                }
                let (snapshot, replay) = {
                    let mut state = self.state.lock().await;
                    state.set_active_task(&session_id, &agent_instance_id)?;
                    let snapshot = state.agent_view_snapshot(&session_id, &agent_instance_id)?;
                    let replay =
                        state.agent_view_replay(&session_id, &agent_instance_id, after_seq)?;
                    (snapshot, replay)
                };
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
