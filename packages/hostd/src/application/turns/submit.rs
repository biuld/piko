use std::path::PathBuf;

use crate::api::{CommandResult, ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::domain::prompts::{
    PromptSnapshotOptions, RunKind, WorldStateFacts, expand_prompt_template, parse_mentions,
    resolve_mention_messages, snapshot_prompt_resources, world_state_context_message,
    world_state_diff_content, world_state_full_content,
};
use crate::ports::AgentRunInput;
use crate::util::{ClientEventSender, now_ms, send_event, storage_error};

impl HostApp {
    /// Resolve the on-disk directory backing this session's AgentInstance
    /// shards. Sessions opened without a configured storage backend (e.g.
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
        let mut paths = self.session_paths.lock().await;
        if let Some(path) = paths.get(session_id) {
            return Ok(path.clone());
        }
        let dir = std::env::temp_dir()
            .join("piko-ephemeral-sessions")
            .join(session_id);
        std::fs::create_dir_all(&dir).map_err(|error| {
            ProtocolError::InvalidCommand(format!(
                "failed to create ephemeral session directory: {error}"
            ))
        })?;
        if self
            .session_store_factory
            .open(&dir)
            .load_manifest()
            .is_err()
        {
            self.session_store_factory
                .create(&dir, session_id.to_string(), cwd.to_string(), now_ms())
                .map_err(storage_error)?;
        }
        paths.insert(session_id.to_string(), dir.clone());
        Ok(dir)
    }

    pub(crate) async fn submit_chat(
        &self,
        command_id: String,
        session_id: String,
        agent_instance_id: String,
        text: String,
        tx: &ClientEventSender,
    ) -> Result<(), ProtocolError> {
        let (turn_id, _) = {
            let mut state = self.state.lock().await;
            state.start_turn(&session_id, &agent_instance_id, &text)?
        };
        self.run_registered_turn(command_id, session_id, turn_id, agent_instance_id, text, tx)
            .await
    }

    async fn run_registered_turn(
        &self,
        command_id: String,
        session_id: String,
        turn_id: String,
        agent_instance_id: String,
        text: String,
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
        let expanded_text = expand_prompt_template(&text, &templates);
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
            let mut state = self.state.lock().await;
            let previous_model = state.record_turn_model(&session_id, active_model.as_ref())?;
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
            let mut state = self.state.lock().await;
            state.record_world_state(&session_id, &world_state_facts)?
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
        let mut prompt_resources = snapshot_prompt_resources(PromptSnapshotOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills: skills.clone(),
            prompt_templates: templates,
            model: world_state_facts.model.clone(),
            previous_model: previous_model.as_ref().map(|model| model.model_id.clone()),
            environment: crate::domain::prompts::EnvironmentSnapshot::capture(),
            cache_policy: self.settings.lock().await.prompt_cache_policy(),
            ..PromptSnapshotOptions::default()
        });
        prompt_resources.world_state = world_state_message;
        // F-03/D-27: expand @path and $skill mentions into retained Context
        // prelude messages; user text stays unchanged for the durable User row.
        let mention_tokens = parse_mentions(&expanded_text);
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
        if is_root && let Some(storage) = &self.storage {
            let _ = storage.set_world_state_baseline(&session_dir, Some(&world_state_facts));
        }
        if let (Some(storage), Some(current)) = (&self.storage, active_model.as_ref()) {
            let changed = previous_model
                .as_ref()
                .is_some_and(|previous| previous != current);
            if changed || previous_model.is_none() {
                let _ = storage.set_last_model(&session_dir, Some(current));
            }
            if changed {
                let parent_id = {
                    let state = self.state.lock().await;
                    state
                        .session(&session_id)
                        .ok()
                        .and_then(|session| session.current_leaf_id.clone())
                };
                if let Ok(entries) = storage.append_config_metadata(
                    &session_dir,
                    parent_id.as_deref(),
                    Some(current.model_id.as_str()),
                    Some(current.provider.as_str()),
                    None,
                    None,
                ) {
                    let mut state = self.state.lock().await;
                    for entry in entries {
                        let _ = state.append_entry(&session_id, entry);
                    }
                }
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
            turn_id = %turn_id,
            agent_instance_id = %agent_instance_id,
            "turn observation loop starting"
        );
        let run = match runner
            .run_agent(AgentRunInput {
                session_id: session_id.clone(),
                operation_id: turn_id.clone(),
                agent_instance_id: agent_instance_id.clone(),
                prompt: expanded_text,
                source_turn_id: Some(turn_id.clone()),
                prompt_resources: Some(prompt_resources),
                cwd: cwd.clone(),
                active_tool_names,
                session_dir: session_dir.clone(),
                resume_agent,
            })
            .await
        {
            Ok(run) => run,
            Err(error) => {
                let failed =
                    self.state
                        .lock()
                        .await
                        .fail_turn(&session_id, &turn_id, error.to_string())?;
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
        let status = self.state.lock().await.apply_turn_input_disposition(
            &session_id,
            &turn_id,
            run.receipt.disposition.clone(),
        )?;
        send_event(
            tx,
            ServerMessage::CommandResponse {
                command_id,
                result: Ok(CommandResult::Empty),
            },
        )
        .await;
        if status == crate::api::TurnStatus::Queued {
            send_event(
                tx,
                ServerMessage::TurnLifecycle(crate::api::TurnEvent::Queued {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    timestamp: now_ms(),
                }),
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
                run,
                tx,
            )
            .await;
        let turn_succeeded = match turn_result {
            Ok(succeeded) => succeeded,
            Err(error) => {
                let cancelled = self
                    .state
                    .lock()
                    .await
                    .turn(&session_id, &turn_id)
                    .is_ok_and(|turn| turn.status == crate::api::TurnStatus::Cancelled);
                if cancelled {
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
