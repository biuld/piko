use std::path::PathBuf;

use piko_protocol::agent_runtime::{SessionEvent, SessionOutput};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use crate::api::{ProtocolError, ServerMessage};
use crate::domain::prompts::skills::load_skills;
use crate::domain::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template, load_context_files,
    load_prompt_templates,
};
use crate::domain::sessions::HostState;
use crate::domain::turns::session_output::{
    apply_message_committed, apply_tool_committed, display_events_from_delta,
    is_root_task_terminal, task_lifecycle_from_task_changed,
};
use crate::domain::turns::{ResumeRootTask, TurnRunInput};

use crate::protocol::{HostServer, now_ms, send_event};

impl HostServer {
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
            let (turn_id, start_events) = state.start_turn(&session_id)?;
            (turn_id, start_events)
        };
        for event in start_events {
            send_event(tx, event);
        }

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id).unwrap_or_default()
        };
        let session_path = if self.storage.is_some() {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        } else {
            None
        };
        let resume_root_task = {
            let state = self.state.lock().await;
            match state.session(&session_id) {
                Ok(session) => {
                    let root_task_id = session
                        .tasks
                        .iter()
                        .find(|(_, task)| task.parent_task_id.is_none())
                        .map(|(task_id, _)| task_id.clone())
                        .or_else(|| session.active_task_id.clone());
                    root_task_id.and_then(|task_id| {
                        let path = session_path.as_ref()?;
                        let repository = crate::infra::storage::TaskRepository::new(path);
                        let recovered = repository.load_task(&session_id, &task_id).ok()?;
                        if recovered.transcript.is_empty() {
                            return None;
                        }
                        Some(ResumeRootTask {
                            task_id,
                            state: piko_protocol::agent_runtime::TaskResumeState {
                                transcript:
                                    crate::infra::storage::transcript_messages_from_recovered(
                                        &recovered,
                                    ),
                                head_message_id: recovered.head_message_id,
                                last_task_seq: recovered.last_task_seq,
                                committed_message_ids: recovered
                                    .transcript
                                    .iter()
                                    .map(|message| message.id.clone())
                                    .collect(),
                            },
                        })
                    })
                }
                Err(_) => None,
            }
        };
        let runner = self.turn_runner.lock().await.clone();
        let work_id = format!("work_{}", uuid::Uuid::new_v4());
        let subscription = runner
            .run_turn_subscription(TurnRunInput {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                work_id,
                prompt: expanded_text,
                system_prompt,
                cwd: cwd.clone(),
                active_tool_names,
                session_dir: session_path.clone(),
                persist_sink: None,
                event_tx: Some(tx.clone()),
                resume_root_task,
            })
            .await?;

        let mut output = subscription.output;
        let mut total_tasks: u32 = 0;
        let mut root_task_id: Option<String> = None;
        let mut turn_done = false;
        let agent_specs = crate::domain::agents::loader::load_agents(&cwd);

        while !turn_done {
            let Some(item) = output.next().await else {
                break;
            };
            let Ok(envelope) = item else {
                continue;
            };

            match envelope.output {
                SessionOutput::Delta(delta_envelope) => {
                    for event in display_events_from_delta(&delta_envelope) {
                        let mut state = self.state.lock().await;

                        if let piko_protocol::DisplayEvent::Finalized {
                            usage: Some(usage), ..
                        } = &event
                            && let Ok(s) = state.session_mut(&session_id)
                        {
                            s.accumulate_usage(usage);
                        }
                        let display_msg = ServerMessage::Display(event.clone());
                        let _ = state.append_agent_view_event(
                            &session_id,
                            event.task_id(),
                            event.agent_id(),
                            display_msg.clone(),
                        )?;

                        let active_task = state
                            .session(&session_id)
                            .ok()
                            .and_then(|s| s.active_task_id.clone());
                        drop(state);

                        if active_task.as_deref() == Some(event.task_id()) {
                            send_event(tx, display_msg);
                        }
                    }
                }
                SessionOutput::Event(event_envelope) => match &event_envelope.event {
                    SessionEvent::TaskChanged { snapshot } => {
                        if snapshot.parent_task_id.is_none() && root_task_id.is_none() {
                            root_task_id = Some(snapshot.task_id.clone());
                        }

                        let lifecycle_msg = task_lifecycle_from_task_changed(
                            &event_envelope,
                            &turn_id,
                            envelope.emitted_at,
                        );
                        if let Some(lifecycle_msg) = lifecycle_msg.clone() {
                            self.handle_task_lifecycle_event(
                                &session_id,
                                &turn_id,
                                &agent_specs,
                                &lifecycle_msg,
                                &mut total_tasks,
                                tx,
                            )
                            .await?;
                        }

                        if root_task_id
                            .as_ref()
                            .is_some_and(|root| is_root_task_terminal(snapshot, root))
                        {
                            turn_done = true;
                        }
                    }
                    SessionEvent::MessageCommitted {
                        message_id,
                        work_id,
                        role,
                    } => {
                        let mut state = self.state.lock().await;
                        apply_message_committed(
                            &self.storage,
                            session_path.as_ref(),
                            &mut state,
                            &session_id,
                            &event_envelope.task_id,
                            &event_envelope.agent_id,
                            message_id,
                            work_id,
                            role,
                        )?;
                    }
                    SessionEvent::ToolCommitted {
                        message_id,
                        work_id,
                        ..
                    } => {
                        let mut state = self.state.lock().await;
                        apply_tool_committed(
                            &self.storage,
                            session_path.as_ref(),
                            &mut state,
                            &session_id,
                            &event_envelope.task_id,
                            &event_envelope.agent_id,
                            message_id,
                            work_id,
                        )?;
                    }
                    _ => {}
                },
            }
        }

        let complete_event = {
            let mut state = self.state.lock().await;
            state.clear_active_turn(&session_id, &turn_id)?;
            ServerMessage::TurnLifecycle(crate::api::TurnEvent::Completed {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                total_tasks: total_tasks.max(1),
                timestamp: now_ms(),
            })
        };
        send_event(tx, complete_event);

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

    async fn handle_task_lifecycle_event(
        &self,
        session_id: &str,
        turn_id: &str,
        agent_specs: &std::collections::HashMap<String, piko_protocol::agents::AgentSpec>,
        lifecycle_msg: &ServerMessage,
        total_tasks: &mut u32,
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        let ServerMessage::TaskLifecycle(event) = lifecycle_msg else {
            return Ok(());
        };

        let mut state = self.state.lock().await;
        match event {
            crate::api::TaskEvent::Created {
                task_id,
                agent_id,
                parent_task_id,
                work_id: _,
                ..
            } => {
                *total_tasks += 1;
                let info = crate::api::AgentInfo {
                    agent_id: agent_id.clone(),
                    task_id: task_id.clone(),
                    parent_task_id: parent_task_id.clone(),
                    name: agent_specs
                        .get(agent_id)
                        .map(|spec| spec.name.clone())
                        .unwrap_or_else(|| agent_id.clone()),
                    role: agent_specs
                        .get(agent_id)
                        .map(|spec| spec.role.clone())
                        .unwrap_or_else(|| "assistant".into()),
                    status: crate::api::AgentStatus::Running,
                };
                if let Ok(s) = state.session_mut(session_id) {
                    if parent_task_id.is_none() {
                        s.active_task_id = Some(task_id.clone());
                    }
                    s.active_agents.insert(task_id.clone(), info.clone());
                }
                if *total_tasks == 1 {
                    send_event(
                        tx,
                        ServerMessage::TurnLifecycle(crate::api::TurnEvent::Started {
                            session_id: session_id.to_string(),
                            turn_id: turn_id.to_string(),
                            root_task_id: task_id.clone(),
                            timestamp: now_ms(),
                        }),
                    );
                }
                let connected_msg = ServerMessage::AgentConnected {
                    agent_id: agent_id.clone(),
                    task_id: task_id.clone(),
                    parent_task_id: parent_task_id.clone(),
                    name: info.name.clone(),
                    role: info.role.clone(),
                };
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    connected_msg.clone(),
                )?;
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    lifecycle_msg.clone(),
                )?;
                drop(state);
                send_event(tx, connected_msg);
                send_event(tx, lifecycle_msg.clone());
            }
            crate::api::TaskEvent::Idle {
                task_id, agent_id, ..
            } => {
                if let Ok(s) = state.session_mut(session_id)
                    && let Some(info) = s.active_agents.get_mut(task_id)
                {
                    info.status = crate::api::AgentStatus::Idle;
                }
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    lifecycle_msg.clone(),
                )?;
                drop(state);
                send_event(tx, lifecycle_msg.clone());
            }
            crate::api::TaskEvent::Completed {
                task_id, agent_id, ..
            }
            | crate::api::TaskEvent::Failed {
                task_id, agent_id, ..
            }
            | crate::api::TaskEvent::Cancelled {
                task_id, agent_id, ..
            } => {
                let (reason, new_status) = match event {
                    crate::api::TaskEvent::Completed { .. } => {
                        ("completed", crate::api::AgentStatus::Completed)
                    }
                    crate::api::TaskEvent::Failed { .. } => {
                        ("failed", crate::api::AgentStatus::Failed)
                    }
                    _ => ("cancelled", crate::api::AgentStatus::Cancelled),
                };
                if let Ok(s) = state.session_mut(session_id)
                    && let Some(info) = s.active_agents.get_mut(task_id)
                {
                    info.status = new_status;
                }
                let disconnected_msg = ServerMessage::AgentDisconnected {
                    agent_id: agent_id.clone(),
                    task_id: task_id.clone(),
                    reason: reason.to_string(),
                };
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    disconnected_msg.clone(),
                )?;
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    lifecycle_msg.clone(),
                )?;
                drop(state);
                send_event(tx, disconnected_msg);
                send_event(tx, lifecycle_msg.clone());
            }
            crate::api::TaskEvent::Closed {
                task_id, agent_id, ..
            } => {
                if let Ok(s) = state.session_mut(session_id)
                    && let Some(info) = s.active_agents.get_mut(task_id)
                {
                    info.status = crate::api::AgentStatus::Closed;
                }
                let disconnected_msg = ServerMessage::AgentDisconnected {
                    agent_id: agent_id.clone(),
                    task_id: task_id.clone(),
                    reason: "closed".to_string(),
                };
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    disconnected_msg.clone(),
                )?;
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    lifecycle_msg.clone(),
                )?;
                drop(state);
                send_event(tx, disconnected_msg);
                send_event(tx, lifecycle_msg.clone());
            }
            crate::api::TaskEvent::Reopened {
                task_id, agent_id, ..
            } => {
                if let Ok(s) = state.session_mut(session_id)
                    && let Some(info) = s.active_agents.get_mut(task_id)
                {
                    info.status = crate::api::AgentStatus::Idle;
                }
                let _ = state.append_agent_view_event(
                    session_id,
                    task_id,
                    agent_id,
                    lifecycle_msg.clone(),
                )?;
                drop(state);
                send_event(tx, lifecycle_msg.clone());
            }
            _ => {
                let task_id = event.task_id();
                let agent_id = match event {
                    crate::api::TaskEvent::Started { agent_id, .. }
                    | crate::api::TaskEvent::Idle { agent_id, .. }
                    | crate::api::TaskEvent::Closed { agent_id, .. }
                    | crate::api::TaskEvent::Reopened { agent_id, .. } => Some(agent_id.clone()),
                    crate::api::TaskEvent::Joined { .. }
                    | crate::api::TaskEvent::Steered { .. } => {
                        state.session(session_id).ok().and_then(|s| {
                            s.active_agents
                                .get(task_id)
                                .map(|info| info.agent_id.clone())
                        })
                    }
                    _ => None,
                };
                if let Some(agent_id) = agent_id {
                    let _ = state.append_agent_view_event(
                        session_id,
                        task_id,
                        &agent_id,
                        lifecycle_msg.clone(),
                    )?;
                }
                drop(state);
                send_event(tx, lifecycle_msg.clone());
            }
        }
        Ok(())
    }
}

fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}
