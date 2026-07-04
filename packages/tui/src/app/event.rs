use piko_protocol::{Command, Message, ServerMessage as Event, SessionSnapshot, SessionTreeEntry};

use crate::{
    app::{
        AppMode, AppState, QueueStatus, ToolStatus, command_id, effect::Effect, flatten_models,
        get_active_branch_entries, short_id,
    },
    config::TuiConfig,
    features::notifications::NotificationLevel,
    features::{
        approval::PendingApproval,
        timeline::{TimelineEntry, ToolEntry},
        tree::session_entry_timeline_text,
    },
    text::{compact_json, message_to_text},
};

impl AppState {
    // ── event application ─────────────────────────────────────────────────────

    pub fn apply_event(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::CommandAccepted { .. }
            | Event::CommandRejected { .. }
            | Event::CommandFailed { .. }
            | Event::CommandResult(piko_protocol::CommandResult::Empty) => {}
            Event::CommandResult(piko_protocol::CommandResult::SessionCreated {
                session_id,
                cwd,
                ..
            }) => {
                self.session.initializing = false;
                self.session.id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                self.push(TimelineEntry::System(format!("created session in {cwd}")));
                self.notify(NotificationLevel::Info, "session created");
                if let Some(name) = self.initial_options.session_name.clone() {
                    effects.push(Effect::send(Command::SessionRename {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                        name,
                    }));
                }
                effects.push(Effect::send(Command::StateSnapshot {
                    command_id: command_id(),
                    session_id: session_id.clone(),
                }));
                if let Some(text) = self.session.pending_turn_text.take() {
                    effects.push(Effect::send(Command::TurnSubmit {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                        text,
                    }));
                    self.status = "submitted turn".to_string();
                }
            }
            Event::CommandResult(piko_protocol::CommandResult::SessionNavigated {
                editor_text,
                ..
            }) => {
                if let Some(text) = editor_text
                    && self.editor.is_empty()
                {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                }
            }
            Event::CommandResult(piko_protocol::CommandResult::SessionOpened {
                session_id,
                snapshot,
                ..
            })
            | Event::CommandResult(piko_protocol::CommandResult::StateSnapshot {
                session_id,
                snapshot,
                ..
            }) => {
                self.session.initializing = false;
                self.session.pending_open_command_id = None;
                self.session.id = Some(session_id.clone());
                self.apply_snapshot(snapshot);
                self.status = format!("session {session_id}");
                self.notify(NotificationLevel::Info, "session opened");
                if self.focus_manager.active_mode() == AppMode::Sessions {
                    self.clear_focus();
                }
                if let Some(text) = self.session.pending_turn_text.take() {
                    effects.push(Effect::send(Command::TurnSubmit {
                        command_id: command_id(),
                        session_id: session_id.clone(),
                        text,
                    }));
                    self.status = "submitted turn".to_string();
                }
            }
            Event::CommandResult(piko_protocol::CommandResult::SessionListed {
                sessions, ..
            }) => {
                self.session.pending_list_command_id = None;
                self.sessions.load(sessions);
                if self.session.continue_requested {
                    self.session.continue_requested = false;
                    if let Some(session_id) = self.sessions.selected_session_id() {
                        effects.push(Effect::send(Command::SessionOpen {
                            command_id: command_id(),
                            session_id,
                            session_path: None,
                        }));
                        self.status = "opening latest session".to_string();
                    } else {
                        effects.push(Effect::send(Command::SessionCreate {
                            command_id: command_id(),
                            cwd: self.cwd.to_string_lossy().into_owned(),
                        }));
                        self.status = "no sessions found; creating session".to_string();
                    }
                    return effects;
                }
                self.push_focus(AppMode::Sessions);
                self.status = format!("{} sessions available", self.sessions.list.items.len());
            }
            Event::Message(piko_protocol::MessageEvent::UserSubmitted {
                message_id, text, ..
            }) => {
                self.timeline.push_user(Some(message_id), text);
            }
            Event::Turn(piko_protocol::TurnEvent::Started {
                turn_id,
                root_task_id,
                ..
            }) => {
                self.session.active_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} running ({root_task_id})");
            }
            Event::Turn(piko_protocol::TurnEvent::Completed { turn_id, .. }) => {
                self.session.active_turn_id = None;
                self.status = format!("turn {turn_id} completed");
            }
            Event::Turn(piko_protocol::TurnEvent::Failed { turn_id, error, .. }) => {
                self.session.active_turn_id = None;
                self.status = format!("turn {turn_id} failed");
                self.push_error(error);
            }
            Event::Turn(piko_protocol::TurnEvent::Cancelled { turn_id, .. }) => {
                self.session.active_turn_id = None;
                self.status = format!("turn {turn_id} cancelled");
            }
            Event::Message(piko_protocol::MessageEvent::TextDelta {
                message_id, delta, ..
            }) => {
                self.timeline.append_text_delta(message_id, delta);
            }
            Event::Message(piko_protocol::MessageEvent::ThinkingDelta {
                message_id,
                delta,
                ..
            }) => {
                self.timeline.append_thinking_delta(message_id, delta);
            }
            Event::Message(piko_protocol::MessageEvent::End {
                message_id,
                stop_reason,
                ..
            }) => {
                self.timeline
                    .finish_assistant_message(message_id, stop_reason);
            }
            Event::Message(piko_protocol::MessageEvent::Finalized {
                message_id,
                content,
                stop_reason,
                ..
            }) => {
                self.timeline
                    .finish_assistant_message(message_id.clone(), stop_reason.clone());
                let message = Message::Assistant {
                    content,
                    api: String::new(),
                    provider: String::new(),
                    model: String::new(),
                    usage: None,
                    stop_reason,
                    error_message: None,
                    timestamp: None,
                };
                self.timeline
                    .complete_assistant_message(message_id, message);
            }
            Event::Message(piko_protocol::MessageEvent::AssistantCompleted {
                message_id,
                message,
                ..
            }) => {
                self.timeline
                    .complete_assistant_message(message_id, message);
            }
            Event::Message(piko_protocol::MessageEvent::ToolCallCommitted {
                parent_message_id,
                message:
                    Message::ToolCall {
                        id,
                        name,
                        arguments,
                        ..
                    },
                ..
            }) => {
                let tool = ToolEntry::new(
                    id,
                    name,
                    ToolStatus::Running,
                    compact_json(&arguments),
                    None,
                    Some(parent_message_id),
                );
                let updated = self.timeline.upsert_tool(tool.clone());
                if !updated {
                    self.push(TimelineEntry::Tool(tool));
                }
            }
            Event::Message(piko_protocol::MessageEvent::ToolCallCommitted { .. }) => {}
            Event::Message(piko_protocol::MessageEvent::ToolResultCommitted {
                message, ..
            }) => self.push_tool_result_message(message),
            Event::Task(piko_protocol::TaskEvent::Created { task_id, .. }) => {
                self.status = format!("task {} created", short_id(&task_id));
            }
            Event::Task(piko_protocol::TaskEvent::Started {
                task_id, agent_id, ..
            }) => {
                self.status = format!(
                    "task {} running on agent {}",
                    short_id(&task_id),
                    short_id(&agent_id)
                );
            }
            Event::Task(piko_protocol::TaskEvent::Cancelled { task_id, .. }) => {
                self.status = format!("task {} cancelled", short_id(&task_id));
            }
            Event::Task(piko_protocol::TaskEvent::Joined { .. })
            | Event::Task(piko_protocol::TaskEvent::Steered { .. }) => {}
            Event::Tool(piko_protocol::ToolEvent::Start {
                tool_call_id,
                tool_name,
                args,
                parent_message_id,
                ..
            }) => {
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
            Event::Tool(piko_protocol::ToolEvent::End {
                tool_call_id,
                tool_name,
                result,
                is_error,
                ..
            }) => {
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
            Event::Approval(piko_protocol::ApprovalEvent::Requested {
                approval_id,
                tool_name,
                tool_args,
                ..
            }) => {
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
            Event::Approval(piko_protocol::ApprovalEvent::Resolved {
                approval_id,
                decision,
                ..
            }) => {
                self.approvals.resolve(&approval_id);
                self.status = format!("approval {approval_id} resolved: {decision:?}");
                if self.approvals.is_empty()
                    && self.focus_manager.active_mode() == AppMode::Approval
                {
                    self.pop_focus();
                }
                if self.approvals.is_empty()
                    && !self.interactions.is_empty()
                    && self.focus_manager.active_mode() != AppMode::ToolInteraction
                {
                    self.push_focus(AppMode::ToolInteraction);
                }
            }
            Event::Interaction(piko_protocol::InteractionEvent::Requested {
                interaction_id,
                title,
                questions,
                require_confirm,
                ..
            }) => {
                self.interactions
                    .push(interaction_id.clone(), title, questions, require_confirm);
                self.status = "user input requested".to_string();
                self.notify(NotificationLevel::Warning, "user input requested");
                if self.approvals.is_empty()
                    && self.focus_manager.active_mode() != AppMode::ToolInteraction
                {
                    self.push_focus(AppMode::ToolInteraction);
                }
            }
            Event::Interaction(piko_protocol::InteractionEvent::Resolved {
                interaction_id,
                status,
                ..
            }) => {
                self.interactions.resolve(&interaction_id);
                self.status = format!("interaction {interaction_id} resolved: {status:?}");
                if self.interactions.is_empty()
                    && self.focus_manager.active_mode() == AppMode::ToolInteraction
                {
                    self.pop_focus();
                }
                if !self.interactions.is_empty()
                    && self.approvals.is_empty()
                    && self.focus_manager.active_mode() != AppMode::ToolInteraction
                {
                    self.push_focus(AppMode::ToolInteraction);
                }
            }
            Event::Task(piko_protocol::TaskEvent::Failed { error, .. }) => self.push_error(error),
            Event::Task(piko_protocol::TaskEvent::Completed {
                agent_id, summary, ..
            }) if !summary.is_empty() && agent_id != "main" => {
                self.push(TimelineEntry::System(summary));
            }
            Event::Task(piko_protocol::TaskEvent::Completed { task_id, .. }) => {
                self.status = format!("task {} completed", short_id(&task_id));
            }
            Event::Queue(piko_protocol::QueueEvent::Updated {
                steer_count,
                follow_up_count,
                next_turn_count,
                steer_preview,
                follow_up_preview,
                ..
            }) => {
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
            Event::Auth(piko_protocol::AuthEvent::LoginDeviceCode {
                provider,
                user_code,
                verification_uri,
            }) => self.push(TimelineEntry::System(format!(
                "{provider} login: open {verification_uri} and enter {user_code}"
            ))),
            Event::Auth(piko_protocol::AuthEvent::LoginSuccess { provider }) => {
                self.push(TimelineEntry::System(format!("{provider} login succeeded")));
            }
            Event::Auth(piko_protocol::AuthEvent::LoginFailed { provider, error }) => {
                self.push_error(format!("{provider} login failed: {error}"));
            }
            Event::Auth(piko_protocol::AuthEvent::LoggedOut { provider }) => {
                self.push(TimelineEntry::System(format!("{provider} logged out")));
            }
            Event::CommandResult(piko_protocol::CommandResult::ModelListed {
                providers, ..
            }) => {
                self.model.providers = providers.clone();
                let provider_names: Vec<String> =
                    providers.iter().map(|p| p.provider.clone()).collect();
                self.auth_selector.reset(&provider_names);
                self.models.load(flatten_models(providers));
                if self.mode != AppMode::AuthSelector {
                    self.push_focus(AppMode::Models);
                }
                self.status = format!("{} models available", self.models.len());
            }
            Event::CommandResult(piko_protocol::CommandResult::CommandCatalogListed {
                commands,
                ..
            }) => {
                self.command_catalog = commands;
                self.refresh_suggestions();
            }
            Event::Model(piko_protocol::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                ..
            }) => {
                self.model.active_model_id = if model_id.is_empty() {
                    None
                } else {
                    Some(model_id.clone())
                };
                self.model.active_provider = if provider.is_empty() {
                    None
                } else {
                    Some(provider.clone())
                };
                if let Some(level) = thinking_level {
                    self.model.active_thinking_level = Some(level.as_str().to_string());
                } else {
                    self.model.active_thinking_level = Some("off".to_string());
                }
                if self.model.active_model_id.is_some() && self.model.active_provider.is_some() {
                    self.status = format!("model {provider}/{model_id}");
                } else {
                    self.status = "no model active".to_string();
                }
            }
            Event::Message(piko_protocol::MessageEvent::Start {
                message_id, role, ..
            }) => {
                if matches!(role, piko_protocol::MessageRole::Assistant) {
                    self.timeline.start_assistant(message_id);
                }
                self.status = format!("message {role:?} started");
            }
            Event::CommandResult(piko_protocol::CommandResult::ConfigEntry {
                namespace,
                value,
            }) => {
                if namespace == "tui" {
                    self.tui_config = TuiConfig::from_hostd_settings(Some(&value));
                    self.editor.configure(&self.tui_config.editor);
                }
            }
        }
        effects
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
                        }
                        Message::ToolCall {
                            id,
                            name,
                            arguments,
                            ..
                        } => {
                            let tool = ToolEntry::new(
                                id,
                                name,
                                ToolStatus::Running,
                                compact_json(&arguments),
                                None,
                                message_entry.parent_id.clone(),
                            );
                            let updated = self.timeline.upsert_tool(tool.clone());
                            if !updated {
                                self.push(TimelineEntry::Tool(tool));
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
                SessionTreeEntry::ToolCall(tool_call) => {
                    let tool = ToolEntry::new(
                        tool_call.tool_call_id,
                        tool_call.tool_name,
                        ToolStatus::Running,
                        compact_json(&tool_call.arguments),
                        None,
                        tool_call.parent_message_id,
                    );
                    let updated = self.timeline.upsert_tool(tool.clone());
                    if !updated {
                        self.push(TimelineEntry::Tool(tool));
                    }
                }
                SessionTreeEntry::ModelChange(change) => {
                    self.model.active_model_id = Some(change.model_id.clone());
                    self.model.active_provider = Some(change.provider.clone());
                    if let Some(text) =
                        session_entry_timeline_text(&SessionTreeEntry::ModelChange(change))
                    {
                        self.push(TimelineEntry::Session(text));
                    }
                }
                SessionTreeEntry::ThinkingLevelChange(change) => {
                    self.model.active_thinking_level = Some(change.thinking_level.clone());
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
            self.session.active_turn_id = Some(turn.turn_id);
        } else {
            self.session.active_turn_id = None;
        }

        self.approvals.clear();
        for approval in snapshot.pending_approvals {
            self.approvals.push(PendingApproval {
                id: approval.approval_id,
                tool_name: "tool".to_string(),
                args: approval.request,
            });
        }
        self.interactions.clear();
        for interaction in snapshot.pending_interactions {
            self.interactions.push(
                interaction.interaction_id,
                interaction.title,
                interaction.questions,
                interaction.require_confirm,
            );
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
