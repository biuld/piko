use std::path::PathBuf;

use piko_orchd_api::AgentInputRuntime;
use piko_protocol::SendAgentInputRequest;

use crate::api::{CommandResult, ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::domain::prompts::{
    PromptSnapshotOptions, RunKind, WorldStateFacts, parse_mentions, resolve_mention_messages,
    snapshot_prompt_resources, world_state_context_message, world_state_diff_content,
    world_state_full_content,
};
use crate::util::{ClientEventSender, now_ms, send_event, storage_error};

impl HostApp {
    /// Resolve the on-disk directory backing this session journal. Sessions
    /// opened without a configured storage backend (e.g.
    /// in-process test harnesses) get a lazily created ephemeral directory
    /// scoped to the process temp dir, cached in `session_paths` so repeated
    /// Turns on the same session reuse one durable store.
    pub(crate) async fn ensure_turn_session_dir(
        &self,
        session_id: &str,
        cwd: &str,
    ) -> Result<PathBuf, ProtocolError> {
        if self.storage.is_some() {
            let paths = self.session_paths.lock().await;
            if let Some(path) = paths.get(session_id) {
                return Ok(path.clone());
            }
        }
        {
            let paths = self.session_paths.lock().await;
            if let Some(path) = paths.get(session_id) {
                return Ok(path.clone());
            }
        }
        let dir = std::env::temp_dir()
            .join("piko-ephemeral-sessions")
            .join(session_id);
        if self
            .session_store_factory
            .open(&dir)
            .load_projection()
            .await
            .is_err()
        {
            self.session_store_factory
                .create(&dir, session_id.to_string(), cwd.to_string(), now_ms())
                .await
                .map_err(storage_error)?;
        }
        self.session_paths
            .lock()
            .await
            .insert(session_id.to_string(), dir.clone());
        Ok(dir)
    }

    pub(crate) async fn submit_chat_with_input_id(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
        input_id: String,
        content: piko_protocol::MessageContent,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        self.run_registered_turn(
            command_id,
            session_id,
            input_id,
            agent_instance_id,
            content,
            tx,
        )
        .await
    }

    async fn run_registered_turn(
        &self,
        command_id: String,
        session_id: String,
        turn_id: String,
        agent_instance_id: String,
        content: piko_protocol::MessageContent,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id)?
        };
        let root_agent_instance_id = format!("agent_{session_id}_root");
        if agent_instance_id == root_agent_instance_id {
            let context_window = self.resolved_model_context_window().await;
            let _ = self
                .compact_session_if_needed(
                    &session_id,
                    &agent_instance_id,
                    context_window,
                    piko_protocol::command::CompactMode::Summarize,
                    false,
                    Some(tx),
                )
                .await;
        }
        let templates = self.prompt_materials.load_prompt_templates(&cwd);
        let expanded_content = super::content::expand_templates(content, &templates);
        let context_files = self.prompt_materials.load_context_files(&cwd);
        let loaded_skills = self.prompt_materials.load_skills(&cwd);
        for diagnostic in &loaded_skills.diagnostics {
            tracing::warn!(
                kind = ?diagnostic.kind,
                path = %diagnostic.path.display(),
                message = %diagnostic.message,
                "skill omitted from prompt snapshot"
            );
        }
        let skills = loaded_skills.skills;
        let active_model = self.active_model.lock().await.clone();
        let (previous_model, continuation) = {
            let state = self.state.lock().await;
            let session = state.session(&session_id)?;
            let previous_model = session.last_model.clone();
            let continuation = state
                .session(&session_id)
                .map(|session| !session.entries.is_empty())
                .unwrap_or(false);
            (previous_model, continuation)
        };
        let model = active_model.as_ref().map(|model| model.model_id.clone());
        // World-state injection (F-04 slice 2) covers the session root agent:
        // the durable baseline is per-session, so comparing across agent
        // instances would mix identities. Child runs (direct chat or
        // multi-agent) keep the agent-spec prompt without a world-state
        // message.
        let is_root = agent_instance_id == root_agent_instance_id;
        let world_state_facts = WorldStateFacts {
            session_id: Some(session_id.clone()),
            agent_instance_id: Some(agent_instance_id.clone()),
            operation_id: Some(turn_id.clone()),
            run_kind: if continuation {
                RunKind::Continuation
            } else {
                RunKind::Initial
            },
            model,
        };
        let previous_world_state = if is_root {
            self.state
                .lock()
                .await
                .session(&session_id)?
                .world_state_baseline
                .clone()
        } else {
            None
        };
        let world_state_message = if is_root {
            match previous_world_state.as_ref() {
                Some(previous) => world_state_diff_content(previous, &world_state_facts),
                None => world_state_full_content(&world_state_facts),
            }
            .map(world_state_context_message)
        } else {
            None
        };
        let todo_feature_on = {
            let settings = self.settings.lock().await;
            let features = crate::domain::features::resolve_features(settings.features.as_ref());
            features.enabled.get("todo").copied().unwrap_or(true)
        };
        let todo_list = if todo_feature_on {
            let state = self.state.lock().await;
            state
                .session(&session_id)
                .ok()
                .and_then(|s| s.todo_lists.get(&agent_instance_id).cloned())
                .filter(|l| !l.items.is_empty())
        } else {
            None
        };
        let mut prompt_resources = snapshot_prompt_resources(PromptSnapshotOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills: skills.clone(),
            prompt_templates: templates,
            model: world_state_facts.model.clone(),
            previous_model: previous_model.as_ref().map(|model| model.model_id.clone()),
            environment: crate::domain::prompts::EnvironmentSnapshot::capture(),
            cache_policy: self.settings.lock().await.prompt_cache_policy(),
            todo_list,
            todo_feature_on,
            ..PromptSnapshotOptions::default()
        });
        prompt_resources.world_state = world_state_message;
        // F-03/D-27: expand @path and $skill mentions into retained Context
        // prelude messages; user text stays unchanged for the durable User row.
        let mention_tokens = parse_mentions(&super::content::plain_text(&expanded_content));
        if !mention_tokens.is_empty() {
            prompt_resources.user_mentions =
                resolve_mention_messages(&mention_tokens, PathBuf::from(&cwd).as_path(), &skills);
        }

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id).unwrap_or_default()
        };
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        // The world-state baseline is durable regardless of model
        // configuration: `model` is one optional fact, not a precondition.
        if is_root {
            if let Some(storage) = &self.storage {
                storage
                    .set_world_state_baseline(&session_dir, Some(&world_state_facts))
                    .await
                    .map_err(storage_error)?;
            }
            self.state
                .lock()
                .await
                .session_mut(&session_id)?
                .world_state_baseline = Some(world_state_facts.clone());
        }
        if let Some(current) = active_model.as_ref() {
            let changed = previous_model
                .as_ref()
                .is_some_and(|previous| previous != current);
            let mut durable_entries = Vec::new();
            if let Some(storage) = &self.storage {
                if changed {
                    let parent_id = {
                        let state = self.state.lock().await;
                        state
                            .session(&session_id)
                            .ok()
                            .and_then(|session| session.current_leaf_id.clone())
                    };
                    durable_entries = storage
                        .append_config_metadata(
                            &session_dir,
                            parent_id.as_deref(),
                            Some(current.model_id.as_str()),
                            Some(current.provider.as_str()),
                            None,
                            None,
                        )
                        .await
                        .map_err(storage_error)?;
                } else if previous_model.is_none() {
                    storage
                        .set_last_model(&session_dir, Some(current))
                        .await
                        .map_err(storage_error)?;
                }
            }
            {
                let mut state = self.state.lock().await;
                state.session_mut(&session_id)?.last_model = Some(current.clone());
                for entry in &durable_entries {
                    state.append_entry(&session_id, entry.clone())?;
                }
            }
            for entry in durable_entries {
                send_event(
                    tx,
                    ServerMessage::SessionEntryCommitted(
                        piko_protocol::SessionEntryCommittedEvent {
                            session_id: session_id.clone(),
                            entry,
                        },
                    ),
                )
                .await;
            }
        }
        let resume_agent = if agent_instance_id == root_agent_instance_id {
            self.resume_root_agent_for_session(&session_id, &session_dir, &root_agent_instance_id)
                .await
        } else {
            None
        };
        let runner = self.turn_runner.lock().await.clone();
        tracing::info!(
            session_id = %session_id,
            input_id = %turn_id,
            agent_instance_id = %agent_instance_id,
            "turn observation loop starting"
        );
        // Bootstrap (idempotently) the runtime session before admission. The
        // durable route registration happens on submit.
        if let Err(error) = runner
            .ensure_session_runtime(&session_id, &cwd, &session_dir, resume_agent.as_ref())
            .await
        {
            let failed = crate::domain::sessions::turn_failed(
                &session_id,
                &turn_id,
                &agent_instance_id,
                error.to_string(),
                piko_protocol::Usage::empty(),
            );
            send_event(
                tx,
                ServerMessage::CommandResponse {
                    command_id,
                    result: Err(error.to_string()),
                },
            )
            .await;
            self.send_turn_terminal(tx, failed).await;
            return Ok(());
        }
        let request = SendAgentInputRequest {
            request_id: turn_id.clone(),
            session_id: session_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            caller_agent_instance_id: None,
            source_turn_id: Some(turn_id.clone()),
            message_id: format!("msg_user_{}", uuid::Uuid::new_v4()),
            content: expanded_content,
            delivery: piko_protocol::AgentInputDelivery::FollowUp,
            prompt_resources: Some(prompt_resources),
            active_tool_names,
        };
        let canonical = piko_protocol::AgentInput::from_request(&request, now_ms());
        let runtime = AgentInputRuntime {
            prompt_resources: request.prompt_resources,
            active_tool_names: request.active_tool_names,
            source_turn_id: request.source_turn_id,
            message_id: Some(request.message_id),
        };
        let receipt = match runner.submit_agent_input(canonical, runtime).await {
            Ok(receipt) => receipt,
            Err(error) => {
                let failed = crate::domain::sessions::turn_failed(
                    &session_id,
                    &turn_id,
                    &agent_instance_id,
                    error.to_string(),
                    piko_protocol::Usage::empty(),
                );
                send_event(
                    tx,
                    ServerMessage::CommandResponse {
                        command_id,
                        result: Err(error.to_string()),
                    },
                )
                .await;
                self.send_turn_terminal(tx, failed).await;
                return Ok(());
            }
        };
        let status = crate::domain::sessions::turn_status_from_disposition(receipt.disposition);
        send_event(
            tx,
            ServerMessage::CommandResponse {
                command_id,
                result: Ok(CommandResult::AgentInputSubmitted {
                    receipt: receipt.clone(),
                    timestamp: now_ms(),
                }),
            },
        )
        .await;
        let (snapshot, agents) = self.session_view(&session_id).await?;
        send_event(
            tx,
            crate::application::sessions::helpers::session_reconciled_message(
                session_id.clone(),
                piko_protocol::ReconcileReason::ExplicitRefresh,
                snapshot,
                agents,
            ),
        )
        .await;
        if status == crate::api::TurnStatus::Queued {
            send_event(
                tx,
                crate::domain::sessions::turn_queued(
                    session_id.clone(),
                    turn_id.clone(),
                    agent_instance_id.clone(),
                ),
            )
            .await;
        }

        let turn_result = self
            .run_turn_observation_loop(
                &runner,
                &session_id,
                &turn_id,
                &agent_instance_id,
                &session_dir,
                &receipt,
                tx,
            )
            .await;
        let turn_succeeded = match turn_result {
            Ok(succeeded) => succeeded,
            Err(error) => {
                if error.to_string().to_ascii_lowercase().contains("cancel") {
                    self.send_turn_terminal(
                        tx,
                        crate::domain::sessions::turn_cancelled(
                            &session_id,
                            &turn_id,
                            &agent_instance_id,
                            piko_protocol::Usage::empty(),
                        ),
                    )
                    .await;
                    return Ok(());
                }
                return Err(error);
            }
        };

        if turn_succeeded && agent_instance_id == root_agent_instance_id {
            let context_window = self.resolved_model_context_window().await;
            let _ = self
                .compact_session_if_needed(
                    &session_id,
                    &agent_instance_id,
                    context_window,
                    piko_protocol::command::CompactMode::Summarize,
                    false,
                    Some(tx),
                )
                .await;
        }
        Ok(())
    }
}
