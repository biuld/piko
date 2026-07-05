use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use crate::api::{
    Message, MessageContent, MessageEntry, ProtocolError, ServerMessage, SessionTreeEntry,
};
use crate::domain::prompts::skills::load_skills;
use crate::domain::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template, load_context_files,
    load_prompt_templates,
};
use crate::domain::sessions::HostState;
use crate::domain::turns::TurnRunInput;

use crate::protocol::{HostServer, now_ms, send_event, storage_error};

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

        let (turn_id, start_events, user_parent_id) = {
            let mut state = self.state.lock().await;
            let (turn_id, start_events) = state.start_turn(&session_id)?;
            let parent_id = state.session(&session_id)?.current_leaf_id.clone();
            (turn_id, start_events, parent_id)
        };
        for event in start_events {
            send_event(tx, event);
        }

        let user_message = Message::User {
            content: MessageContent::String(expanded_text.clone()),
            timestamp: Some(now_ms()),
        };
        let user_entry = if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                storage
                    .append_message(&path, user_parent_id.as_deref(), &user_message, None)
                    .map_err(storage_error)?
            } else {
                SessionTreeEntry::Message(MessageEntry {
                    id: format!("msg_{}", uuid::Uuid::new_v4()),
                    parent_id: user_parent_id.clone(),
                    timestamp: now_ms().to_string(),
                    agent_id: None,
                    message: user_message,
                })
            }
        } else {
            SessionTreeEntry::Message(MessageEntry {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                parent_id: user_parent_id.clone(),
                timestamp: now_ms().to_string(),
                agent_id: None,
                message: user_message,
            })
        };
        let _user_message_id = user_entry.id().to_string();
        {
            let mut state = self.state.lock().await;
            state.append_entry(&session_id, user_entry)?;
        }

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let cwd = {
            let state = self.state.lock().await;
            state.session_cwd(&session_id).unwrap_or_default()
        };
        let runner = self.turn_runner.lock().await.clone();
        let mut channels = runner
            .run_turn_channels(TurnRunInput {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                prompt: expanded_text,
                system_prompt,
                cwd: cwd.clone(),
                active_tool_names,
            })
            .await?;
        let session_path = if self.storage.is_some() {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        } else {
            None
        };

        let mut display_stream = channels
            .display_stream()
            .ok_or_else(|| ProtocolError::InvalidCommand("display stream unavailable".into()))?;
        let mut persist_stream = channels
            .persist_stream()
            .ok_or_else(|| ProtocolError::InvalidCommand("persist stream unavailable".into()))?;
        let mut lifecycle_stream = channels
            .lifecycle_stream()
            .ok_or_else(|| ProtocolError::InvalidCommand("lifecycle stream unavailable".into()))?;

        let mut total_tasks: u32 = 0;
        let mut display_done = false;
        let mut persist_done = false;
        let mut lifecycle_done = false;

        while !display_done || !persist_done || !lifecycle_done {
            tokio::select! {
                display_event = display_stream.next(), if !display_done => {
                    match display_event {
                        Some(event) => {
                            let event = (*event).clone();
                            
                            let mut state = self.state.lock().await;
                            
                            // Track usage
                            if let piko_protocol::DisplayEvent::Finalized {
                                usage: Some(usage), ..
                            } = &event {
                                if let Ok(s) = state.session_mut(&session_id) {
                                    s.accumulate_usage(usage);
                                }
                            }
                            
                            // Filter Display events by active_agent_id (TUI subscription)
                            let active_agent = state.session(&session_id)
                                .ok()
                                .and_then(|s| s.active_agent_id.clone())
                                .unwrap_or_else(|| "main".to_string());
                            drop(state);
                            
                            if event.agent_id() == active_agent {
                                send_event(tx, ServerMessage::Display(event));
                            }
                        }
                        None => display_done = true,
                    }
                }
                persist_event = persist_stream.next(), if !persist_done => {
                    match persist_event {
                        Some(event) => {
                            let event = (*event).clone();
                            let mut state = self.state.lock().await;
                            persist_from_event(
                                &self.storage,
                                session_path.as_ref(),
                                &mut state,
                                &session_id,
                                &event,
                            )?;
                        }
                        None => persist_done = true,
                    }
                }
                lifecycle_event = lifecycle_stream.next(), if !lifecycle_done => {
                    match lifecycle_event {
                        Some(event) => {
                            let event = (*event).clone();
                            let lifecycle_msg = match &event {
                                piko_protocol::LifecycleEvent::Task(t) => {
                                    ServerMessage::TaskLifecycle(t.clone())
                                }
                                piko_protocol::LifecycleEvent::Turn(t) => {
                                    ServerMessage::TurnLifecycle(t.clone())
                                }
                            };
                            let mut state = self.state.lock().await;
                            match &event {
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Created {
                                        task_id: _, agent_id, parent_task_id, ..
                                    },
                                ) => {
                                    total_tasks += 1;
                                    let info = crate::api::AgentInfo {
                                        agent_id: agent_id.clone(),
                                        parent_agent_id: parent_task_id.clone(),
                                        name: agent_id.clone(),
                                        role: "assistant".into(),
                                        status: crate::api::AgentStatus::Running,
                                    };
                                    if let Ok(s) = state.session_mut(&session_id) {
                                        s.active_agents.insert(agent_id.clone(), info.clone());
                                    }
                                    drop(state);
                                    send_event(
                                        tx,
                                        ServerMessage::AgentConnected {
                                            agent_id: agent_id.clone(),
                                            parent_agent_id: parent_task_id.clone(),
                                            name: agent_id.clone(),
                                            role: "assistant".into(),
                                        },
                                    );
                                    send_event(tx, lifecycle_msg);
                                }
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Completed { agent_id, .. }
                                )
                                | piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Failed { agent_id, .. }
                                )
                                | piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Cancelled { agent_id, .. }
                                ) => {
                                    let (reason, new_status) = match &event {
                                        piko_protocol::LifecycleEvent::Task(
                                            crate::api::TaskEvent::Completed { .. },
                                        ) => ("completed", crate::api::AgentStatus::Completed),
                                        piko_protocol::LifecycleEvent::Task(
                                            crate::api::TaskEvent::Failed { .. },
                                        ) => ("failed", crate::api::AgentStatus::Failed),
                                        _ => ("cancelled", crate::api::AgentStatus::Cancelled),
                                    };
                                    if let Ok(s) = state.session_mut(&session_id)
                                        && let Some(info) = s.active_agents.get_mut(agent_id)
                                    {
                                        info.status = new_status;
                                    }
                                    drop(state);
                                    send_event(
                                        tx,
                                        ServerMessage::AgentDisconnected {
                                            agent_id: agent_id.clone(),
                                            reason: reason.to_string(),
                                        },
                                    );
                                    send_event(tx, lifecycle_msg);
                                }
                                _ => {
                                    drop(state);
                                    send_event(tx, lifecycle_msg);
                                }
                            }
                        }
                        None => lifecycle_done = true,
                    }
                }
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
}

fn persist_from_event(
    storage: &Option<crate::infra::storage::JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    event: &piko_protocol::PersistEvent,
) -> Result<(), ProtocolError> {
    let parent_id = state.session(session_id)?.current_leaf_id.clone();
    let (message_id, message, agent_id) = match event {
        piko_protocol::PersistEvent::Finalized {
            message_id,
            message,
            agent_id,
            ..
        } => (message_id.clone(), message.clone(), Some(agent_id.clone())),
        piko_protocol::PersistEvent::ToolResultCommitted {
            message_id,
            message,
            agent_id,
            ..
        } => (message_id.clone(), message.clone(), Some(agent_id.clone())),
        piko_protocol::PersistEvent::ToolCallCommitted {
            message_id,
            message,
            agent_id,
            ..
        } => (message_id.clone(), message.clone(), Some(agent_id.clone())),
        _ => return Ok(()),
    };
    let timestamp = message_timestamp(&message).to_string();
    let entry = SessionTreeEntry::Message(MessageEntry {
        id: message_id.clone(),
        parent_id,
        timestamp,
        agent_id,
        message,
    });

    if let (Some(storage), Some(path)) = (storage, session_path) {
        storage
            .append_entry(path, &entry, None)
            .map_err(storage_error)?;
    }
    state.append_entry(session_id, entry)
}

fn message_timestamp(message: &Message) -> &i64 {
    const DEFAULT: i64 = 0;
    match message {
        Message::User { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::Assistant { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolCall { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
        Message::ToolResult { timestamp, .. } => timestamp.as_ref().unwrap_or(&DEFAULT),
    }
}

fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}
