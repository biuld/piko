use piko_protocol::{Command, ContentBlock, Event, Message, SessionSnapshot, SessionTreeEntry};

use crate::{
    app::{
        AppMode, AppState, QueueStatus, ToolStatus, command_id, flatten_models,
        get_active_branch_entries, short_id,
    },
    config::TuiConfig,
    features::notifications::NotificationLevel,
    features::{
        approval::PendingApproval,
        timeline::{TimelineEntry, ToolEntry},
        tree::session_entry_timeline_text,
    },
    host::HostdClient,
    text::{compact_json, message_to_text},
};

impl AppState {
    // ── event application ─────────────────────────────────────────────────────

    #[allow(clippy::needless_option_as_deref)]
    pub fn apply_event(&mut self, mut host: Option<&mut HostdClient>, event: Event) {
        match event {
            Event::SessionCreated {
                session_id, cwd, ..
            } => {
                self.session_initializing = false;
                self.session_id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                self.push(TimelineEntry::System(format!("created session in {cwd}")));
                self.notify(NotificationLevel::Info, "session created");
                if let Some(name) = self.initial_options.session_name.clone()
                    && let Some(host) = host.as_deref_mut()
                {
                    let _ = host.send(Command::SessionRename {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                        name,
                    });
                }
                if let Some(host) = host.as_deref_mut() {
                    let _ = host.send(Command::StateSnapshot {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                    });
                    if let Some(text) = self.pending_turn_text.take() {
                        let _ = host.send(Command::TurnSubmit {
                            command_id: command_id(),
                            session_id: session_id.clone(),
                            text,
                        });
                        self.status = "submitted turn".to_string();
                    }
                }
            }
            Event::SessionOpened {
                session_id,
                snapshot,
                ..
            }
            | Event::StateSnapshot {
                session_id,
                snapshot,
                ..
            } => {
                self.session_initializing = false;
                self.pending_session_open_command_id = None;
                self.session_id = Some(session_id.clone());
                self.apply_snapshot(snapshot);
                self.status = format!("session {session_id}");
                self.notify(NotificationLevel::Info, "session opened");
                if self.focus_manager.active_mode() == AppMode::Sessions {
                    self.clear_focus();
                }
                if let (Some(text), Some(host)) =
                    (self.pending_turn_text.take(), host.as_deref_mut())
                {
                    let _ = host.send(Command::TurnSubmit {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                        text,
                    });
                    self.status = "submitted turn".to_string();
                }
            }
            Event::SessionListed { sessions, .. } => {
                self.pending_session_list_command_id = None;
                self.sessions.load(sessions);
                if self.continue_session {
                    self.continue_session = false;
                    if let Some(session_id) = self.sessions.selected_session_id(&self.filter_text) {
                        if let Some(host) = host.as_deref_mut() {
                            let _ = host.send(Command::SessionOpen {
                                command_id: command_id(),
                                session_id,
                                session_path: None,
                            });
                        }
                        self.status = "opening latest session".to_string();
                    } else {
                        if let Some(host) = host.as_deref_mut() {
                            let _ = host.send(Command::SessionCreate {
                                command_id: command_id(),
                                cwd: self.cwd.to_string_lossy().into_owned(),
                            });
                        }
                        self.status = "no sessions found; creating session".to_string();
                    }
                    return;
                }
                self.push_focus(AppMode::Sessions);
                self.status = format!("{} sessions available", self.sessions.list.items.len());
            }
            Event::UserMessageSubmitted {
                message_id, text, ..
            } => {
                self.timeline.push_user(Some(message_id), text);
            }
            Event::TurnStarted {
                turn_id,
                root_task_id,
                ..
            } => {
                self.active_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} running ({root_task_id})");
            }
            Event::TurnCompleted { turn_id, .. } => {
                self.active_turn_id = None;
                self.status = format!("turn {turn_id} completed");
            }
            Event::TurnFailed { turn_id, error, .. } => {
                self.active_turn_id = None;
                self.status = format!("turn {turn_id} failed");
                self.push_error(error);
            }
            Event::TurnCancelled { turn_id, .. } => {
                self.active_turn_id = None;
                self.status = format!("turn {turn_id} cancelled");
            }
            Event::TextDelta {
                message_id, delta, ..
            } => {
                self.timeline.append_text_delta(message_id, delta);
            }
            Event::ThinkingDelta {
                message_id, delta, ..
            } => {
                self.timeline.append_thinking_delta(message_id, delta);
            }
            Event::MessageEnd {
                message_id,
                stop_reason,
                ..
            } => {
                self.timeline
                    .finish_assistant_message(message_id, stop_reason);
            }
            Event::AssistantMessageCompleted {
                message_id,
                message,
                ..
            } => {
                self.timeline
                    .complete_assistant_message(message_id, message);
            }
            Event::ToolResultCommitted { message, .. } => self.push_tool_result_message(message),
            Event::TaskCreated {
                task_id,
                agent_id,
                parent_task_id,
                prompt,
                ..
            } => {
                let parent = parent_task_id
                    .map(|id| format!(" parent {}", short_id(&id)))
                    .unwrap_or_default();
                self.status = format!("task {} created", short_id(&task_id));
                self.push(TimelineEntry::Session(format!(
                    "task {} created for agent {}{parent}: {}",
                    short_id(&task_id),
                    short_id(&agent_id),
                    prompt
                )));
            }
            Event::TaskStarted {
                task_id, agent_id, ..
            } => {
                self.status = format!(
                    "task {} running on agent {}",
                    short_id(&task_id),
                    short_id(&agent_id)
                );
            }
            Event::TaskCancelled {
                task_id, agent_id, ..
            } => {
                self.status = format!("task {} cancelled", short_id(&task_id));
                self.push(TimelineEntry::Session(format!(
                    "task {} cancelled on agent {}",
                    short_id(&task_id),
                    short_id(&agent_id)
                )));
            }
            Event::TaskJoined {
                task_id,
                parent_task_id,
                result,
                ..
            } => {
                self.push(TimelineEntry::Session(format!(
                    "task {} joined parent {}: {}",
                    short_id(&task_id),
                    short_id(&parent_task_id),
                    compact_json(&result)
                )));
            }
            Event::TaskSteered {
                task_id,
                source_task_id,
                message,
                ..
            } => {
                self.push(TimelineEntry::Session(format!(
                    "task {} steered by {}: {}",
                    short_id(&task_id),
                    short_id(&source_task_id),
                    message
                )));
            }
            Event::ToolStart {
                tool_call_id,
                tool_name,
                args,
                parent_message_id,
                ..
            } => {
                let tool = ToolEntry::new(
                    tool_call_id,
                    tool_name,
                    ToolStatus::Running,
                    compact_json(&args),
                    None,
                    parent_message_id,
                );
                let updated = self.timeline.upsert_tool(tool.clone());
                if !updated {
                    self.push(TimelineEntry::Tool(tool));
                }
            }
            Event::ToolEnd {
                tool_call_id,
                tool_name,
                result,
                is_error,
                ..
            } => {
                let status = if is_error {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Completed
                };
                let mut tool = self
                    .timeline
                    .tool_calls
                    .iter()
                    .find(|t| t.id == tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        ToolEntry::new(
                            tool_call_id.clone(),
                            tool_name.clone(),
                            ToolStatus::Running,
                            String::new(),
                            None,
                            None,
                        )
                    });
                tool.name = tool_name;
                tool.status = status;
                tool.result = Some(compact_json(&result));
                let updated = self.timeline.upsert_tool(tool.clone());
                if !updated {
                    self.push(TimelineEntry::Tool(tool));
                }
            }
            Event::ApprovalRequested {
                approval_id,
                tool_name,
                tool_args,
                ..
            } => {
                self.approvals.push(PendingApproval {
                    id: approval_id.clone(),
                    tool_name: tool_name.clone(),
                    args: tool_args,
                });
                self.status = format!("approval requested for {tool_name}");
                self.notify(
                    NotificationLevel::Warning,
                    format!("approval requested for {tool_name}"),
                );
                if self.focus_manager.active_mode() != AppMode::Approval {
                    self.push_focus(AppMode::Approval);
                }
            }
            Event::ApprovalResolved {
                approval_id,
                decision,
                ..
            } => {
                self.approvals.resolve(&approval_id);
                self.status = format!("approval {approval_id} resolved: {decision:?}");
                if self.approvals.is_empty()
                    && self.focus_manager.active_mode() == AppMode::Approval
                {
                    self.pop_focus();
                }
            }
            Event::TaskFailed { error, .. } => self.push_error(error),
            Event::TaskCompleted { summary, .. } if !summary.is_empty() => {
                self.push(TimelineEntry::System(summary));
            }
            Event::TaskCompleted { task_id, .. } => {
                self.status = format!("task {} completed", short_id(&task_id));
            }
            Event::QueueUpdate {
                steer_count,
                follow_up_count,
                next_turn_count,
                steer_preview,
                follow_up_preview,
                ..
            } => {
                self.queue_status = QueueStatus {
                    steer_count,
                    follow_up_count,
                    next_turn_count,
                    steer_preview,
                    follow_up_preview,
                };
                self.status = format!(
                    "queue steer={steer_count} follow_up={follow_up_count} next_turn={next_turn_count}"
                );
            }
            Event::AuthLoginDeviceCode {
                provider,
                user_code,
                verification_uri,
            } => self.push(TimelineEntry::System(format!(
                "{provider} login: open {verification_uri} and enter {user_code}"
            ))),
            Event::AuthLoginSuccess { provider } => {
                self.push(TimelineEntry::System(format!("{provider} login succeeded")));
            }
            Event::AuthLoginFailed { provider, error } => {
                self.push_error(format!("{provider} login failed: {error}"));
            }
            Event::AuthLoggedOut { provider } => {
                self.push(TimelineEntry::System(format!("{provider} logged out")));
            }
            Event::ModelListed { providers, .. } => {
                self.models.load(flatten_models(providers));
                self.push_focus(AppMode::Models);
                self.status = format!("{} models available", self.models.list.items.len());
            }
            Event::CommandCatalogListed { commands, .. } => {
                self.command_catalog = commands;
                self.refresh_suggestions();
            }
            Event::ModelConfigChanged {
                model_id,
                provider,
                thinking_level,
                ..
            } => {
                self.active_model_id = Some(model_id.clone());
                self.active_provider = Some(provider.clone());
                if let Some(level) = thinking_level {
                    self.active_thinking_level = Some(level.as_str().to_string());
                } else {
                    self.active_thinking_level = Some("off".to_string());
                }
                self.status = format!("model {provider}/{model_id}");
            }
            Event::MessageStart {
                message_id, role, ..
            } => {
                if matches!(role, piko_protocol::MessageRole::Assistant) {
                    self.timeline.start_assistant(message_id);
                }
                self.status = format!("message {role:?} started");
            }
            Event::ConfigEntry { namespace, value } => {
                if namespace == "tui" {
                    self.tui_config = TuiConfig::from_hostd_settings(Some(&value));
                    self.editor.configure(&self.tui_config.editor);
                }
            }
        }
    }

    // ── snapshot application ──────────────────────────────────────────────────

    fn apply_snapshot(&mut self, snapshot: SessionSnapshot) {
        self.timeline.clear();
        self.queue_status = QueueStatus::default();
        self.tree
            .load(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        let active_entries =
            get_active_branch_entries(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        for entry in active_entries {
            match entry {
                SessionTreeEntry::Message(message_entry) => {
                    let text = message_to_text(&message_entry.message);
                    let message_id = message_entry.id.clone();
                    match message_entry.message {
                        Message::User { .. } => self.timeline.push_user(Some(message_id), text),
                        Message::Assistant { content, .. } => {
                            let assistant_message = Message::Assistant {
                                content: content.clone(),
                                api: String::new(),
                                provider: String::new(),
                                model: String::new(),
                                usage: None,
                                stop_reason: None,
                                error_message: None,
                                timestamp: None,
                            };
                            self.timeline
                                .complete_assistant_message(message_id.clone(), assistant_message);
                            for block in content {
                                if let ContentBlock::ToolCall {
                                    id,
                                    name,
                                    arguments,
                                    ..
                                } = block
                                {
                                    let tool = ToolEntry::new(
                                        id,
                                        name,
                                        ToolStatus::Running,
                                        compact_json(&arguments),
                                        None,
                                        Some(message_id.clone()),
                                    );
                                    let updated = self.timeline.upsert_tool(tool.clone());
                                    if !updated {
                                        self.push(TimelineEntry::Tool(tool));
                                    }
                                }
                            }
                        }
                        Message::ToolResult {
                            tool_call_id,
                            tool_name,
                            content: _,
                            is_error,
                            ..
                        } => {
                            let status = if is_error.unwrap_or(false) {
                                ToolStatus::Failed
                            } else {
                                ToolStatus::Completed
                            };
                            let mut tool = self
                                .timeline
                                .tool_calls
                                .iter()
                                .find(|t| t.id == tool_call_id)
                                .cloned()
                                .unwrap_or_else(|| {
                                    ToolEntry::new(
                                        tool_call_id.clone(),
                                        tool_name.clone().unwrap_or_else(|| "tool".into()),
                                        ToolStatus::Running,
                                        String::new(),
                                        None,
                                        None,
                                    )
                                });
                            if let Some(name) = tool_name {
                                tool.name = name;
                            }
                            tool.status = status;
                            tool.result = Some(text);
                            let updated = self.timeline.upsert_tool(tool.clone());
                            if updated {
                                continue;
                            }
                            self.push(TimelineEntry::Tool(tool));
                        }
                    }
                }
                SessionTreeEntry::ModelChange(change) => {
                    self.active_model_id = Some(change.model_id.clone());
                    self.active_provider = Some(change.provider.clone());
                    if let Some(text) =
                        session_entry_timeline_text(&SessionTreeEntry::ModelChange(change))
                    {
                        self.push(TimelineEntry::Session(text));
                    }
                }
                SessionTreeEntry::ThinkingLevelChange(change) => {
                    self.active_thinking_level = Some(change.thinking_level.clone());
                    if let Some(text) =
                        session_entry_timeline_text(&SessionTreeEntry::ThinkingLevelChange(change))
                    {
                        self.push(TimelineEntry::Session(text));
                    }
                }
                other => {
                    if let Some(text) = session_entry_timeline_text(&other) {
                        self.push(TimelineEntry::Session(text));
                    }
                }
            }
        }

        if let Some(turn) = snapshot.active_turn {
            self.active_turn_id = Some(turn.turn_id);
        } else {
            self.active_turn_id = None;
        }

        self.approvals.clear();
        for approval in snapshot.pending_approvals {
            self.approvals.push(PendingApproval {
                id: approval.approval_id,
                tool_name: "tool".to_string(),
                args: approval.request,
            });
        }
    }

    fn push_tool_result_message(&mut self, message: Message) {
        let text = message_to_text(&message);
        let Message::ToolResult {
            tool_call_id,
            tool_name,
            is_error,
            ..
        } = message
        else {
            if !text.is_empty() {
                self.push(TimelineEntry::Session(format!("tool result: {text}")));
            }
            return;
        };
        let status = if is_error.unwrap_or(false) {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        };
        let mut tool = self
            .timeline
            .tool_calls
            .iter()
            .find(|t| t.id == tool_call_id)
            .cloned()
            .unwrap_or_else(|| {
                ToolEntry::new(
                    tool_call_id.clone(),
                    tool_name.clone().unwrap_or_else(|| "tool".to_string()),
                    ToolStatus::Running,
                    String::new(),
                    None,
                    None,
                )
            });
        if let Some(name) = tool_name {
            tool.name = name;
        }
        tool.status = status;
        tool.result = Some(text);
        let updated = self.timeline.upsert_tool(tool.clone());
        if !updated {
            self.push(TimelineEntry::Tool(tool));
        }
    }
}
