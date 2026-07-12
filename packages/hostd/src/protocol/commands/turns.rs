use std::path::PathBuf;

use piko_protocol::agent_runtime::{SessionEvent, SessionOutput};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_stream::StreamExt;

use crate::api::{ProtocolError, ServerMessage};
use crate::domain::prompts::skills::load_skills;
use crate::domain::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template, load_context_files,
    load_prompt_templates,
};
use crate::domain::sessions::HostState;
use crate::domain::turns::session_output::{
    is_execution_terminal, realtime_message_from_delta, record_committed_message,
};
use crate::domain::turns::{ResumeRootAgent, TurnRunInput};
use crate::infra::storage::SessionStore;

use crate::protocol::{HostServer, now_ms, send_event};

impl HostServer {
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
                .map_err(crate::protocol::storage_error)?;
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
        let resume_root_agent = {
            let state = self.state.lock().await;
            match state.session(&session_id) {
                Ok(session) => {
                    let session_transcript =
                        crate::infra::storage::transcript_messages_from_session_entries(
                            &session.entries,
                        );
                    if !session_transcript.is_empty() {
                        let transcript_seq = SessionStore::new(&session_dir)
                            .load_agent(&session_id, &root_agent_instance_id)
                            .ok()
                            .map(|recovered| recovered.last_transcript_seq)
                            .unwrap_or_else(|| {
                                session
                                    .entries
                                    .iter()
                                    .filter_map(|entry| match entry {
                                        piko_protocol::SessionTreeEntry::Message(message)
                                            if message.agent_instance_id
                                                == root_agent_instance_id =>
                                        {
                                            Some(message.transcript_seq)
                                        }
                                        _ => None,
                                    })
                                    .max()
                                    .unwrap_or(0)
                            });
                        let head_message_id = session
                            .task_heads
                            .get(&root_agent_instance_id)
                            .cloned()
                            .or_else(|| session.current_leaf_id.clone());
                        Some(ResumeRootAgent {
                            agent_instance_id: root_agent_instance_id.clone(),
                            state: piko_protocol::agent_runtime::AgentResumeState {
                                head_message_id,
                                transcript_seq,
                                committed_message_ids: session
                                    .entries
                                    .iter()
                                    .filter_map(|entry| match entry {
                                        piko_protocol::SessionTreeEntry::Message(message) => {
                                            Some(message.id.clone())
                                        }
                                        _ => None,
                                    })
                                    .collect(),
                                transcript: session_transcript,
                            },
                        })
                    } else {
                        SessionStore::new(&session_dir)
                            .load_agent(&session_id, &root_agent_instance_id)
                            .ok()
                            .filter(|recovered| !recovered.transcript.is_empty())
                            .map(|recovered| ResumeRootAgent {
                                agent_instance_id: root_agent_instance_id.clone(),
                                state: piko_protocol::agent_runtime::AgentResumeState {
                                    transcript: recovered
                                        .transcript
                                        .iter()
                                        .map(|message| message.message.clone())
                                        .collect(),
                                    head_message_id: recovered.head_message_id.clone(),
                                    transcript_seq: recovered.last_transcript_seq,
                                    committed_message_ids: recovered
                                        .transcript
                                        .iter()
                                        .map(|message| message.id.clone())
                                        .collect(),
                                },
                            })
                    }
                }
                Err(_) => None,
            }
        };
        let runner = self.turn_runner.lock().await.clone();
        let work_id = format!("work_{}", uuid::Uuid::new_v4());
        let mut root_task_id: Option<String> = resume_root_agent
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
                root_task_id: root_task_id.clone().unwrap_or_default(),
                timestamp: now_ms(),
            }),
        );
        let (ui_event_tx, mut ui_event_rx) = unbounded_channel();
        let subscription = runner
            .run_turn_subscription(TurnRunInput {
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

        let mut output = subscription.output;
        let mut total_tasks: u32 = 0;
        let mut turn_done = false;
        let mut root_terminal_status: Option<piko_protocol::ExecutionStatus> = None;
        let agent_specs = crate::domain::agents::loader::load_agents(&cwd);

        while !turn_done {
            tokio::select! {
                ui_event = ui_event_rx.recv() => {
                    if let Some(event) = ui_event {
                        send_event(tx, event);
                    }
                }
                item = output.next() => {
                    let Some(item) = item else {
                        tracing::warn!(session_id, "session output closed; reconnecting");
                        let (runtime_snapshot, recovered) =
                            runner.recover_session_subscription(&session_id).await?;
                        let (snapshot, agents) = {
                            let state = self.state.lock().await;
                            (
                                state.snapshot(&session_id)?,
                                state.get_agent_list(&session_id),
                            )
                        };
                        send_event(
                            tx,
                            ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
                                session_id: session_id.clone(),
                                reason: piko_protocol::ReconcileReason::Reconnect,
                                cursor: runtime_snapshot.cursor,
                                snapshot,
                                agents,
                            }),
                        );
                        output = recovered.output;
                        continue;
                    };
                    let envelope = match item {
                        Ok(envelope) => envelope,
                        Err(orchd_api::SessionStreamError::SnapshotRequired { reason }) => {
                            tracing::warn!(session_id, ?reason, "reconciling exhausted session output");
                            let (runtime_snapshot, recovered) =
                                runner.recover_session_subscription(&session_id).await?;
                            let (snapshot, agents) = {
                                let state = self.state.lock().await;
                                (
                                    state.snapshot(&session_id)?,
                                    state.get_agent_list(&session_id),
                                )
                            };
                            send_event(
                                tx,
                                ServerMessage::SessionReconciled(piko_protocol::SessionReconciledEvent {
                                    session_id: session_id.clone(),
                                    reason: piko_protocol::ReconcileReason::RetentionExhausted,
                                    cursor: runtime_snapshot.cursor,
                                    snapshot,
                                    agents,
                                }),
                            );
                            output = recovered.output;
                            continue;
                        }
                        Err(error) => {
                            return Err(ProtocolError::ObservationFailed(format!(
                                "session {session_id}: {error}"
                            )));
                        }
                    };

                    match envelope.output {
                        SessionOutput::Delta(delta_envelope) => {
                            let Some(event) = realtime_message_from_delta(&session_id, &delta_envelope)
                            else {
                                tracing::warn!(
                                    session_id,
                                    execution_id = %delta_envelope.execution_id,
                                    agent_id = %delta_envelope.agent_id,
                                    delta_seq = delta_envelope.delta_seq,
                                    "dropping realtime delta without message identity"
                                );
                                continue;
                            };
                            send_event(tx, ServerMessage::RealtimeMessage(event));
                        }
                        SessionOutput::Event(event_envelope) => match &event_envelope.event {
                            SessionEvent::ExecutionChanged { snapshot } => {
                                if root_task_id.is_none() {
                                    root_task_id = Some(snapshot.execution_id.clone());
                                }

                                tracing::info!(
                                    session_id = %session_id,
                                    turn_id = %turn_id,
                                    execution_id = %snapshot.execution_id,
                                    status = ?snapshot.status,
                                    "observed execution changed"
                                );

                                self.apply_execution_observation(
                                    &runner,
                                    &cwd,
                                    &session_id,
                                    &agent_specs,
                                    snapshot,
                                    &mut total_tasks,
                                    tx,
                                )
                                .await?;

                                if is_execution_terminal(&snapshot.status) {
                                    tracing::info!(
                                        session_id = %session_id,
                                        turn_id = %turn_id,
                                        execution_id = %snapshot.execution_id,
                                        status = ?snapshot.status,
                                        "root execution reached terminal status; ending turn"
                                    );
                                    root_terminal_status = Some(snapshot.status.clone());
                                    turn_done = true;
                                }
                            }
                            SessionEvent::MessageCommitted {
                                message_id,
                                work_id: _,
                                role,
                            } => {
                                tracing::info!(
                                    session_id = %session_id,
                                    turn_id = %turn_id,
                                    agent_instance_id = %event_envelope.agent_instance_id,
                                    message_id = %message_id,
                                    ?role,
                                    "observed message committed"
                                );
                                let committed = {
                                    let mut state = self.state.lock().await;
                                    let store = SessionStore::new(&session_dir);
                                    record_committed_message(
                                        &mut state,
                                        Some(&store),
                                        &session_id,
                                        &event_envelope.agent_instance_id,
                                        message_id,
                                    )?
                                };
                                if let Some(committed) = committed {
                                    send_event(tx, ServerMessage::TranscriptCommitted(committed));
                                } else {
                                    tracing::error!(
                                        session_id = %session_id,
                                        turn_id = %turn_id,
                                        agent_instance_id = %event_envelope.agent_instance_id,
                                        message_id = %message_id,
                                        "committed projection missing; aborting turn observation"
                                    );
                                    return Err(ProtocolError::ObservationFailed(format!(
                                        "committed projection {message_id} missing for agent {}",
                                        event_envelope.agent_instance_id
                                    )));
                                }
                            }
                            SessionEvent::ToolCommitted {
                                message_id,
                                work_id: _,
                                ..
                            } => {
                                let committed = {
                                    let mut state = self.state.lock().await;
                                    let store = SessionStore::new(&session_dir);
                                    record_committed_message(
                                        &mut state,
                                        Some(&store),
                                        &session_id,
                                        &event_envelope.agent_instance_id,
                                        message_id,
                                    )?
                                };
                                if let Some(committed) = committed {
                                    send_event(tx, ServerMessage::TranscriptCommitted(committed));
                                } else {
                                    return Err(ProtocolError::ObservationFailed(format!(
                                        "committed tool projection {message_id} missing for agent {}",
                                        event_envelope.agent_instance_id
                                    )));
                                }
                            }
                            _ => {}
                        },
                    }
                }
            }
        }

        let complete_event = {
            let mut state = self.state.lock().await;
            let still_active = state
                .session(&session_id)
                .ok()
                .and_then(|s| s.active_turn_id.clone())
                .as_deref()
                == Some(turn_id.as_str());
            if !still_active {
                // Already finalized (e.g. TurnCancel cleared active_turn_id).
                None
            } else {
                match root_terminal_status {
                    Some(piko_protocol::ExecutionStatus::Failed) => {
                        Some(state.fail_turn(&session_id, &turn_id, "execution failed")?)
                    }
                    Some(piko_protocol::ExecutionStatus::Cancelled) => {
                        Some(state.cancel_turn(&session_id, &turn_id)?)
                    }
                    _ => {
                        state.clear_active_turn(&session_id, &turn_id)?;
                        Some(ServerMessage::TurnLifecycle(
                            crate::api::TurnEvent::Completed {
                                session_id: session_id.clone(),
                                turn_id: turn_id.clone(),
                                total_tasks: total_tasks.max(1),
                                timestamp: now_ms(),
                            },
                        ))
                    }
                }
            }
        };
        let turn_succeeded = matches!(
            (&complete_event, &root_terminal_status),
            (
                Some(ServerMessage::TurnLifecycle(
                    crate::api::TurnEvent::Completed { .. }
                )),
                None | Some(piko_protocol::ExecutionStatus::Succeeded)
            )
        );
        if let Some(complete_event) = complete_event {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                total_tasks,
                "turn observation loop finished; emitting terminal"
            );
            send_event(tx, complete_event);
        } else {
            tracing::info!(
                session_id = %session_id,
                turn_id = %turn_id,
                "turn observation loop finished; turn already terminal"
            );
        }

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

    #[allow(clippy::too_many_arguments)]
    async fn apply_execution_observation(
        &self,
        runner: &std::sync::Arc<dyn crate::domain::turns::TurnRunner>,
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

fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}
