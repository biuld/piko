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
    fn with_task_timeline(
        &mut self,
        task_id: &str,
        apply: impl FnOnce(&mut crate::features::timeline::Timeline),
    ) {
        let is_active = self
            .agent_panel
            .active_task_id
            .as_deref()
            .is_none_or(|active| active == task_id);
        if is_active {
            if self.agent_panel.active_task_id.is_none() {
                self.agent_panel.active_task_id = Some(task_id.to_string());
            }
            apply(&mut self.timeline);
        } else {
            apply(
                self.task_timelines
                    .entry(task_id.to_string())
                    .or_insert_with(crate::features::timeline::Timeline::new),
            );
        }
    }

    fn accepts_session(&self, session_id: &str) -> bool {
        self.session
            .id
            .as_deref()
            .is_none_or(|current| current == session_id)
    }

    fn select_task_timeline(&mut self, task_id: &str) {
        if self.agent_panel.active_task_id.as_deref() == Some(task_id) {
            return;
        }
        if let Some(previous) = self.agent_panel.active_task_id.replace(task_id.to_string()) {
            let previous_timeline = std::mem::replace(
                &mut self.timeline,
                self.task_timelines
                    .remove(task_id)
                    .unwrap_or_else(crate::features::timeline::Timeline::new),
            );
            self.task_timelines.insert(previous, previous_timeline);
        } else {
            self.timeline = self
                .task_timelines
                .remove(task_id)
                .unwrap_or_else(crate::features::timeline::Timeline::new);
        }
    }

    // ── event application ─────────────────────────────────────────────────────

    pub fn apply_event(&mut self, event: Event) -> Vec<Effect> {
        let mut effects = Vec::new();
        match event {
            Event::TranscriptCommitted(committed) => {
                if !self.accepts_session(&committed.session_id) {
                    return effects;
                }
                let task_id = committed.task_id.clone();
                let mut consistent = true;
                self.with_task_timeline(&task_id, |timeline| {
                    consistent = timeline.apply_committed(committed);
                });
                if !consistent && let Some(session_id) = self.session.id.clone() {
                    effects.push(Effect::send(Command::StateSnapshot {
                        command_id: command_id(),
                        session_id,
                    }));
                }
            }
            Event::RealtimeMessage(realtime) => {
                if !self.accepts_session(&realtime.session_id) {
                    return effects;
                }
                let task_id = realtime.task_id.clone();
                self.with_task_timeline(&task_id, |timeline| timeline.apply_realtime(realtime));
            }
            Event::SessionReconciled(reconciled) => {
                if self.accepts_session(&reconciled.session_id) {
                    self.session.id = Some(reconciled.session_id.clone());
                    self.apply_snapshot(reconciled.snapshot);
                    self.agent_panel.agents.clear();
                    for agent in reconciled.agents {
                        self.agent_panel
                            .upsert_agent(crate::features::agent_status::AgentEntry {
                                agent_id: agent.agent_id,
                                task_id: agent.task_id,
                                name: agent.name,
                                parent_task_id: agent.parent_task_id,
                                status: agent.status,
                            });
                    }
                    let active_task_id = self
                        .agent_panel
                        .agents
                        .iter()
                        .find(|agent| agent.parent_task_id.is_none())
                        .or_else(|| self.agent_panel.agents.first())
                        .map(|agent| agent.task_id.clone());
                    if let Some(active_task_id) = active_task_id {
                        self.select_task_timeline(&active_task_id);
                    }
                }
            }
            Event::AgentChanged(agent) => {
                self.agent_panel
                    .upsert_agent(crate::features::agent_status::AgentEntry {
                        agent_id: agent.agent_id,
                        task_id: agent.task_id,
                        name: agent.name,
                        parent_task_id: agent.parent_task_id,
                        status: agent.status,
                    });
            }
            Event::ToolExecution(piko_protocol::ToolExecutionEvent::Started {
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
                if !self.timeline.upsert_tool(tool.clone()) {
                    self.push(TimelineEntry::Tool(tool));
                }
            }
            Event::ToolExecution(piko_protocol::ToolExecutionEvent::Ended {
                tool_call_id,
                tool_name,
                result,
                is_error,
                ..
            }) => {
                let mut tool = self
                    .timeline
                    .tool_calls
                    .iter()
                    .find(|tool| tool.id == tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| {
                        ToolEntry::new(
                            tool_call_id,
                            tool_name,
                            ToolStatus::Running,
                            String::new(),
                            None,
                            None,
                        )
                    });
                tool.status = if is_error {
                    ToolStatus::Failed
                } else {
                    ToolStatus::Completed
                };
                tool.result = Some(compact_json(&result));
                if !self.timeline.upsert_tool(tool.clone()) {
                    self.push(TimelineEntry::Tool(tool));
                }
            }
            Event::Interaction(piko_protocol::InteractionEvent::Requested {
                interaction_id,
                title,
                questions,
                require_confirm,
                auto_resolution_ms,
                ..
            }) => {
                self.interactions
                    .push(interaction_id, title, questions, require_confirm);
                if auto_resolution_ms.is_none() {
                    self.push_focus(AppMode::ToolInteraction);
                }
            }
            Event::Interaction(piko_protocol::InteractionEvent::Resolved {
                interaction_id,
                status: _,
                ..
            }) => {
                self.interactions.resolve(&interaction_id);
                if self.interactions.is_empty()
                    && self.focus_manager.active_mode() == AppMode::ToolInteraction
                {
                    self.clear_focus();
                }
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::Empty),
                ..
            }
            | Event::CommandResponse { result: Err(_), .. } => {}
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::SessionCreated {
                        session_id, cwd, ..
                    }),
                ..
            } => {
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
                    let submit_command_id = command_id();
                    self.session.pending_turn_command_id = Some(submit_command_id.clone());
                    effects.push(Effect::send(Command::TurnSubmit {
                        command_id: submit_command_id,
                        session_id: session_id.clone(),
                        text,
                    }));
                    self.status = "submitted turn".to_string();
                }
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::SessionNavigated { editor_text, .. }),
                ..
            } => {
                if let Some(text) = editor_text
                    && self.editor.is_empty()
                {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                }
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::SessionOpened {
                        session_id,
                        snapshot: _,
                        ..
                    }),
                ..
            } => {
                self.session.initializing = false;
                self.session.pending_open_command_id = None;
                self.session.id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                self.notify(NotificationLevel::Info, "session opened");
                if self.focus_manager.active_mode() == AppMode::Sessions {
                    self.clear_focus();
                }
                if let Some(text) = self.session.pending_turn_text.take() {
                    let submit_command_id = command_id();
                    self.session.pending_turn_command_id = Some(submit_command_id.clone());
                    effects.push(Effect::send(Command::TurnSubmit {
                        command_id: submit_command_id,
                        session_id,
                        text,
                    }));
                    self.status = "submitted turn".to_string();
                }
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::StateSnapshot {
                        session_id,
                        snapshot,
                        ..
                    }),
                ..
            } => {
                self.session.initializing = false;
                self.session.pending_open_command_id = None;
                self.session.id = Some(session_id.clone());
                self.apply_snapshot(snapshot);
                self.status = format!("session {session_id} refreshed");
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::SessionListed { sessions, .. }),
                ..
            } => {
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
            Event::TurnLifecycle(piko_protocol::TurnEvent::Started {
                turn_id,
                root_task_id,
                ..
            }) => {
                self.session.pending_turn_command_id = None;
                self.session.active_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} running ({root_task_id})");
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { turn_id, .. }) => {
                if self.session.active_turn_id.as_deref() == Some(&turn_id) {
                    self.session.pending_turn_command_id = None;
                    self.session.active_turn_id = None;
                }
                self.status = format!("turn {turn_id} completed");
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Failed { turn_id, error, .. }) => {
                if self.session.active_turn_id.as_deref() == Some(&turn_id) {
                    self.session.pending_turn_command_id = None;
                    self.session.active_turn_id = None;
                }
                self.status = format!("turn {turn_id} failed");
                self.push_error(error);
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Cancelled { turn_id, .. }) => {
                if self.session.active_turn_id.as_deref() == Some(&turn_id) {
                    self.session.pending_turn_command_id = None;
                    self.session.active_turn_id = None;
                }
                self.status = format!("turn {turn_id} cancelled");
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Created { task_id, .. }) => {
                self.status = format!("task {} created", short_id(&task_id));
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Started {
                task_id, agent_id, ..
            }) => {
                self.status = format!(
                    "task {} running on agent {}",
                    short_id(&task_id),
                    short_id(&agent_id)
                );
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Running;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Idle { task_id, .. }) => {
                self.status = format!("task {} idle", short_id(&task_id));
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Idle;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Cancelled { task_id, .. }) => {
                self.status = format!("task {} cancelled", short_id(&task_id));
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Cancelled;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Closed { task_id, .. }) => {
                self.status = format!("task {} closed", short_id(&task_id));
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Closed;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Reopened { task_id, .. }) => {
                self.status = format!("task {} reopened", short_id(&task_id));
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Idle;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Joined { .. })
            | Event::TaskLifecycle(piko_protocol::TaskEvent::Steered { .. }) => {}
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
            Event::TaskLifecycle(piko_protocol::TaskEvent::Failed { task_id, error, .. }) => {
                self.push_error(error);
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Failed;
                }
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Completed { task_id, .. }) => {
                self.status = format!("task {} completed", short_id(&task_id));
                if let Some(existing) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.task_id == task_id)
                {
                    existing.status = piko_protocol::AgentStatus::Completed;
                }
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
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::ModelListed { providers, .. }),
                ..
            } => {
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
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::CommandCatalogListed { commands, .. }),
                ..
            } => {
                self.command_catalog = commands;
                self.refresh_suggestions();
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::AgentSpecListed { agents, .. }),
                ..
            } => {
                self.agents.load(agents);
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::AgentListed { agents, .. }),
                ..
            } => {
                self.agent_panel.agents.clear();
                for a in &agents {
                    self.agent_panel
                        .upsert_agent(crate::features::agent_status::AgentEntry {
                            agent_id: a.agent_id.clone(),
                            task_id: a.task_id.clone(),
                            name: a.name.clone(),
                            parent_task_id: a.parent_task_id.clone(),
                            status: a.status.clone(),
                        });
                }
                self.status = format!("{} agents active", agents.len());
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::AgentSubscribed {
                        task_id,
                        agent_id,
                        snapshot,
                        replay,
                        ..
                    }),
                ..
            } => {
                self.select_task_timeline(&task_id);
                let events = if snapshot.events.is_empty() {
                    replay
                } else {
                    snapshot.events
                };
                for event in events {
                    effects.extend(self.apply_event(*event.message));
                }
                self.status = format!("subscribed to agent {agent_id} task {task_id}");
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
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }),
                ..
            } => {
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
        self.task_timelines.clear();
        self.agent_panel.active_task_id = None;
        self.queue_status = QueueStatus::default();
        self.tree
            .load(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        let active_entries =
            get_active_branch_entries(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        for entry in active_entries {
            match entry {
                SessionTreeEntry::Message(message_entry) => {
                    let task_id = message_entry.task_id.clone();
                    let committed = piko_protocol::TranscriptCommittedEvent {
                        session_id: snapshot.session_id.clone(),
                        task_id: message_entry.task_id,
                        agent_id: message_entry.agent_id,
                        work_id: message_entry.work_id,
                        message_id: message_entry.id,
                        task_seq: message_entry.task_seq,
                        message: message_entry.message,
                    };
                    self.with_task_timeline(&task_id, |timeline| {
                        timeline.apply_committed(committed);
                    });
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

    #[allow(dead_code)]
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
