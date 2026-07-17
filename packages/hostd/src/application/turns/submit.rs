use std::path::PathBuf;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::api::{CommandResult, ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::domain::prompts::{
    BuildSystemPromptOptions, expand_prompt_template, snapshot_prompt_resources,
};
use crate::ports::AgentRunInput;
use crate::util::{now_ms, send_event, storage_error};

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
        tx: &UnboundedSender<ServerMessage>,
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
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id)?
        };
        let templates = self.prompt_materials.load_prompt_templates(&cwd);
        let expanded_text = expand_prompt_template(&text, &templates);
        let context_files = self.prompt_materials.load_context_files(&cwd);
        let skills = self.prompt_materials.load_skills(&cwd).skills;
        let prompt_resources = snapshot_prompt_resources(BuildSystemPromptOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills,
            prompt_templates: templates,
            ..BuildSystemPromptOptions::default()
        });

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id).unwrap_or_default()
        };
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        let root_agent_instance_id = format!("agent_{session_id}_root");
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
        let (ui_event_tx, ui_event_rx) = unbounded_channel();
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
                ui_event_tx,
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
                );
                send_event(tx, failed);
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
        );
        if status == crate::api::TurnStatus::Queued {
            send_event(
                tx,
                ServerMessage::TurnLifecycle(crate::api::TurnEvent::Queued {
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    timestamp: now_ms(),
                }),
            );
        }

        let turn_result = self
            .run_turn_observation_loop(
                &runner,
                &session_id,
                &turn_id,
                &agent_instance_id,
                &session_dir,
                run,
                ui_event_rx,
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
            let context_window = {
                let settings = self.settings.lock().await;
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.reserve_tokens)
                    .unwrap_or(16384)
                    + settings
                        .compaction
                        .as_ref()
                        .and_then(|c| c.keep_recent_tokens)
                        .unwrap_or(20000)
            };
            self.compact_session_if_needed(
                &turn_id,
                &session_id,
                &agent_instance_id,
                context_window,
                tx,
            )
            .await;
        }
        Ok(())
    }
}
