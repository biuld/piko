use std::path::PathBuf;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::adapters::prompts::{load_context_files, load_prompt_templates, load_skills};
use crate::api::{ProtocolError, ServerMessage};
use crate::application::host_app::HostApp;
use crate::domain::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template,
};
use crate::infra::storage::SessionStore;
use crate::ports::TurnRunInput;
use crate::util::{now_ms, send_event, storage_error};

use super::queue::drain_one_queued;

impl HostApp {
    /// Resolve the on-disk directory backing this session's AgentInstance
    /// shards. Sessions opened without a configured storage backend (e.g.
    /// in-process test harnesses) get a lazily created ephemeral directory
    /// scoped to the process temp dir, cached in `session_paths` so repeated
    /// Turns on the same session reuse one durable store.
    async fn ensure_turn_session_dir(
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
        if SessionStore::new(&dir).load_manifest().is_err() {
            SessionStore::create_session(&dir, session_id.to_string(), cwd.to_string(), now_ms())
                .map_err(storage_error)?;
        }
        paths.insert(session_id.to_string(), dir.clone());
        Ok(dir)
    }

    pub(crate) async fn apply_turn_submit(
        &self,
        _command_id: String,
        session_id: String,
        text: String,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id)?
        };
        let templates = load_prompt_templates(&cwd);
        let expanded_text = expand_prompt_template(&text, &templates);
        let context_files = load_context_files(&cwd);
        let skills = load_skills(&cwd).skills;
        let system_prompt = build_system_prompt(BuildSystemPromptOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills,
            prompt_templates: templates,
            ..BuildSystemPromptOptions::default()
        });

        let (turn_id, start_events) = {
            let mut state = self.state.lock().await;
            match state.start_turn(&session_id) {
                Ok(started) => started,
                Err(ProtocolError::ActiveTurnExists(_)) => {
                    // Keep root work serial: queue until the active turn terminals,
                    // then drain_one_queued re-enters apply_turn_submit.
                    let queue_ev = state.push_next_turn(&session_id, &text);
                    tracing::info!(
                        session_id = %session_id,
                        "turn submit queued; prior turn still active"
                    );
                    drop(state);
                    send_event(tx, queue_ev.into());
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
        };
        for event in start_events {
            send_event(tx, event);
        }

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id).unwrap_or_default()
        };
        let session_dir = self.ensure_turn_session_dir(&session_id, &cwd).await?;
        let root_agent_instance_id = format!("agent_{session_id}_root");
        let resume_root_agent = self
            .resume_root_agent_for_session(&session_id, &session_dir, &root_agent_instance_id)
            .await;
        let runner = self.turn_runner.lock().await.clone();
        let work_id = format!("work_{}", uuid::Uuid::new_v4());
        let root_task_id = resume_root_agent
            .as_ref()
            .map(|agent| agent.agent_instance_id.clone());
        tracing::info!(
            session_id = %session_id,
            turn_id = %turn_id,
            work_id = %work_id,
            "turn observation loop starting"
        );
        // Emit TurnStarted as soon as the turn is accepted. Follow-up turns reuse the
        // root task (no TaskEvent::Created), so gating Started on Created left the TUI
        // without active_turn_id and suppressed the agent spinner.
        send_event(
            tx,
            ServerMessage::TurnLifecycle(crate::api::TurnEvent::Started {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                root_task_id: root_task_id.unwrap_or_default(),
                timestamp: now_ms(),
            }),
        );
        let (ui_event_tx, ui_event_rx) = unbounded_channel();
        let turn_run = runner
            .run_turn(TurnRunInput {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                work_id,
                prompt: expanded_text,
                system_prompt,
                cwd: cwd.clone(),
                active_tool_names,
                session_dir: session_dir.clone(),
                ui_event_tx,
                resume_root_agent,
            })
            .await?;

        let turn_succeeded = self
            .run_turn_observation_loop(
                &runner,
                &session_id,
                &turn_id,
                &session_dir,
                turn_run,
                ui_event_rx,
                tx,
            )
            .await?;

        if !turn_succeeded {
            return Ok(());
        }

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
        self.compact_session_if_needed(&_command_id, &session_id, context_window, tx)
            .await;

        let mut queued: Vec<String> = Vec::new();
        let mut state = self.state.lock().await;
        while let Some(next_text) = drain_one_queued(&mut state, &session_id) {
            queued.push(next_text);
        }
        drop(state);

        for next_text in queued {
            {
                let state = self.state.lock().await;
                let queue_event: ServerMessage = state.build_queue_update(&session_id).into();
                drop(state);
                send_event(tx, queue_event);
            }
            Box::pin(self.apply_turn_submit(
                format!("auto-{}", uuid::Uuid::new_v4()),
                session_id.clone(),
                next_text,
                tx,
            ))
            .await?;
        }
        Ok(())
    }
}
