use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use crate::api::{
    Message, MessageEntry, ProtocolError, ServerMessage, SessionTreeEntry, ToolCallEntry,
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
        drop(channels);

        let mut total_tasks: u32 = 0;
        let mut display_done = false;
        let mut persist_done = false;
        let mut lifecycle_done = false;
        let mut pending_message_commits = Vec::new();
        let agent_specs = crate::domain::agents::loader::load_agents(&cwd);

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

                            // Forward only the foreground agent to the TUI.
                            let active_task = state.session(&session_id)
                                .ok()
                                .and_then(|s| s.active_task_id.clone());
                            drop(state);

                            if active_task.as_deref() == Some(event.task_id()) {
                                send_event(tx, display_msg);
                            }
                        }
                        None => display_done = true,
                    }
                }
                persist_event = persist_stream.next(), if !persist_done => {
                    match persist_event {
                        Some(event) => {
                            let event = (*event).clone();
                            if let (Some(path), Some(task_id)) =
                                (session_path.as_ref(), persist_message_task_id(&event))
                                && !path.join("tasks").join(format!("{task_id}.jsonl")).exists()
                            {
                                pending_message_commits.push(event);
                                continue;
                            }
                            let mut state = self.state.lock().await;
                            persist_from_event(
                                &self.storage,
                                session_path.as_ref(),
                                &mut state,
                                &session_id,
                                &event,
                            )?;
                            if matches!(event, piko_protocol::PersistEvent::TaskEventCommitted(_))
                                && let Some(path) = session_path.as_ref()
                            {
                                let mut index = 0;
                                while index < pending_message_commits.len() {
                                    let ready = persist_message_task_id(&pending_message_commits[index])
                                        .is_some_and(|task_id| path.join("tasks").join(format!("{task_id}.jsonl")).exists());
                                    if ready {
                                        let pending = pending_message_commits.remove(index);
                                        persist_from_event(
                                            &self.storage,
                                            session_path.as_ref(),
                                            &mut state,
                                            &session_id,
                                            &pending,
                                        )?;
                                    } else {
                                        index += 1;
                                    }
                                }
                            }
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
                                        task_id,
                                        agent_id,
                                        parent_task_id,
                                        turn_id,
                                        ..
                                    },
                                ) => {
                                    total_tasks += 1;
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
                                    if let Ok(s) = state.session_mut(&session_id) {
                                        if parent_task_id.is_none() {
                                            s.active_task_id = Some(task_id.clone());
                                        }
                                        s.active_agents.insert(task_id.clone(), info.clone());
                                    }
                                    if total_tasks == 1 {
                                        send_event(
                                            tx,
                                            ServerMessage::TurnLifecycle(
                                                crate::api::TurnEvent::Started {
                                                    session_id: session_id.clone(),
                                                    turn_id: turn_id.clone(),
                                                    root_task_id: task_id.clone(),
                                                    timestamp: now_ms(),
                                                },
                                            ),
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
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        connected_msg.clone(),
                                    )?;
                                    let _ = state.append_agent_view_event(
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        lifecycle_msg.clone(),
                                    )?;
                                    drop(state);
                                    send_event(tx, connected_msg);
                                    send_event(tx, lifecycle_msg);
                                }
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Idle {
                                        task_id, agent_id, ..
                                    }
                                ) => {
                                    if let Ok(s) = state.session_mut(&session_id)
                                        && let Some(info) = s.active_agents.get_mut(task_id)
                                    {
                                        info.status = crate::api::AgentStatus::Idle;
                                    }
                                    let _ = state.append_agent_view_event(
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        lifecycle_msg.clone(),
                                    )?;
                                    drop(state);
                                    send_event(tx, lifecycle_msg);
                                }
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Completed {
                                        task_id, agent_id, ..
                                    }
                                )
                                | piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Failed {
                                        task_id, agent_id, ..
                                    }
                                )
                                | piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Cancelled {
                                        task_id, agent_id, ..
                                    }
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
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        disconnected_msg.clone(),
                                    )?;
                                    let _ = state.append_agent_view_event(
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        lifecycle_msg.clone(),
                                    )?;
                                    drop(state);
                                    send_event(tx, disconnected_msg);
                                    send_event(tx, lifecycle_msg);
                                }
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Closed {
                                        task_id, agent_id, ..
                                    },
                                ) => {
                                    if let Ok(s) = state.session_mut(&session_id)
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
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        disconnected_msg.clone(),
                                    )?;
                                    let _ = state.append_agent_view_event(
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        lifecycle_msg.clone(),
                                    )?;
                                    drop(state);
                                    send_event(tx, disconnected_msg);
                                    send_event(tx, lifecycle_msg);
                                }
                                piko_protocol::LifecycleEvent::Task(
                                    crate::api::TaskEvent::Reopened {
                                        task_id, agent_id, ..
                                    },
                                ) => {
                                    if let Ok(s) = state.session_mut(&session_id)
                                        && let Some(info) = s.active_agents.get_mut(task_id)
                                    {
                                        info.status = crate::api::AgentStatus::Idle;
                                    }
                                    let _ = state.append_agent_view_event(
                                        &session_id,
                                        task_id,
                                        agent_id,
                                        lifecycle_msg.clone(),
                                    )?;
                                    drop(state);
                                    send_event(tx, lifecycle_msg);
                                }
                                _ => {
                                    if let piko_protocol::LifecycleEvent::Task(task_event) = &event {
                                        let task_id = task_event.task_id();
                                        let agent_id = match task_event {
                                            crate::api::TaskEvent::Started { agent_id, .. }
                                            | crate::api::TaskEvent::Idle { agent_id, .. }
                                            | crate::api::TaskEvent::Closed { agent_id, .. }
                                            | crate::api::TaskEvent::Reopened { agent_id, .. } => {
                                                Some(agent_id.clone())
                                            }
                                            crate::api::TaskEvent::Joined { .. }
                                            | crate::api::TaskEvent::Steered { .. } => state
                                                .session(&session_id)
                                                .ok()
                                                .and_then(|s| {
                                                    s.active_agents
                                                        .get(task_id)
                                                        .map(|info| info.agent_id.clone())
                                                }),
                                            _ => None,
                                        };
                                        if let Some(agent_id) = agent_id {
                                            let _ = state.append_agent_view_event(
                                                &session_id,
                                                task_id,
                                                &agent_id,
                                                lifecycle_msg.clone(),
                                            )?;
                                        }
                                    }
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

        if !pending_message_commits.is_empty() {
            return Err(ProtocolError::InvalidCommand(format!(
                "{} message commit(s) arrived before their task was created",
                pending_message_commits.len()
            )));
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

fn persist_message_task_id(event: &piko_protocol::PersistEvent) -> Option<&str> {
    match event {
        piko_protocol::PersistEvent::UserCommitted { task_id, .. }
        | piko_protocol::PersistEvent::Finalized { task_id, .. }
        | piko_protocol::PersistEvent::ToolCallCommitted { task_id, .. }
        | piko_protocol::PersistEvent::ToolResultCommitted { task_id, .. } => Some(task_id),
        piko_protocol::PersistEvent::TaskEventCommitted(_) => None,
    }
}

fn persist_from_event(
    storage: &Option<crate::infra::storage::JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    event: &piko_protocol::PersistEvent,
) -> Result<(), ProtocolError> {
    if let piko_protocol::PersistEvent::TaskEventCommitted(task_event) = event {
        if let (Some(storage), Some(path)) = (storage, session_path) {
            storage
                .apply_task_event(path, task_event)
                .map_err(storage_error)?;
        }
        return Ok(());
    }

    let task_id = match event {
        piko_protocol::PersistEvent::UserCommitted { task_id, .. }
        | piko_protocol::PersistEvent::Finalized { task_id, .. }
        | piko_protocol::PersistEvent::ToolResultCommitted { task_id, .. }
        | piko_protocol::PersistEvent::ToolCallCommitted { task_id, .. } => task_id.clone(),
        piko_protocol::PersistEvent::TaskEventCommitted(_) => unreachable!(),
    };
    let parent_id = state.session(session_id)?.task_heads.get(&task_id).cloned();
    let (message_id, message, agent_id, task_id, work_id) = match event {
        piko_protocol::PersistEvent::UserCommitted {
            message_id,
            message,
            agent_id,
            task_id,
            work_id,
            ..
        } => (
            message_id.clone(),
            message.clone(),
            agent_id.clone(),
            task_id.clone(),
            work_id.clone(),
        ),
        piko_protocol::PersistEvent::Finalized {
            message_id,
            message,
            agent_id,
            task_id,
            work_id,
            ..
        } => (
            message_id.clone(),
            message.clone(),
            agent_id.clone(),
            task_id.clone(),
            work_id.clone(),
        ),
        piko_protocol::PersistEvent::ToolResultCommitted {
            message_id,
            message,
            agent_id,
            task_id,
            work_id,
            ..
        } => (
            message_id.clone(),
            message.clone(),
            agent_id.clone(),
            task_id.clone(),
            work_id.clone(),
        ),
        piko_protocol::PersistEvent::ToolCallCommitted {
            message_id,
            message,
            agent_id,
            task_id,
            work_id,
            ..
        } => (
            message_id.clone(),
            message.clone(),
            agent_id.clone(),
            task_id.clone(),
            work_id.clone(),
        ),
        _ => return Ok(()),
    };
    let storage_agent_id = agent_id.clone();
    let committed_task_id = task_id.clone();
    let task_seq = if let Some(path) = session_path {
        crate::infra::storage::TaskRepository::new(path)
            .next_task_seq(session_id, &task_id)
            .map_err(storage_error)?
    } else {
        state
            .session(session_id)?
            .entries
            .iter()
            .filter(|entry| {
                matches!(entry, SessionTreeEntry::Message(message) if message.task_id == task_id)
                    || matches!(entry, SessionTreeEntry::ToolCall(tool) if tool.task_id.as_deref() == Some(task_id.as_str()))
            })
            .count() as u64
            + 1
    };
    let timestamp = message_timestamp(&message).to_string();
    let entry = match message {
        Message::ToolCall {
            id,
            name,
            arguments,
            model,
            provider,
            ..
        } => SessionTreeEntry::ToolCall(ToolCallEntry {
            id: message_id.clone(),
            parent_id,
            timestamp,
            agent_id: Some(agent_id),
            task_id: Some(task_id),
            tool_call_id: id,
            tool_name: name,
            arguments,
            parent_message_id: match event {
                piko_protocol::PersistEvent::ToolCallCommitted {
                    parent_message_id, ..
                } => Some(parent_message_id.clone()),
                _ => None,
            },
            model,
            provider,
        }),
        message => SessionTreeEntry::Message(MessageEntry {
            id: message_id.clone(),
            parent_id,
            timestamp,
            agent_id,
            task_id,
            work_id,
            task_seq,
            message,
        }),
    };

    if let (Some(storage), Some(path)) = (storage, session_path) {
        storage
            .append_entry(path, &entry, Some(&storage_agent_id))
            .map_err(storage_error)?;
    }
    state.append_task_entry(session_id, &committed_task_id, entry)
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
