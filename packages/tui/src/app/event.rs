use piko_protocol::{Command, ServerMessage as Event, SessionSnapshot, SessionTreeEntry};

use crate::{
    app::{
        AppMode, AppState, QueueStatus, ToolStatus, command_id, effect::Effect, flatten_models,
        get_active_branch_entries,
    },
    config::TuiConfig,
    features::notifications::NotificationLevel,
    features::{
        approval::PendingApproval,
        timeline::{TimelineEntry, ToolEntry},
        tree::session_entry_timeline_text,
    },
    text::compact_json,
};

impl AppState {
    fn with_agent_timeline(
        &mut self,
        agent_instance_id: &str,
        apply: impl FnOnce(&mut crate::features::timeline::Timeline),
    ) {
        let is_active = self
            .agent_panel
            .active_agent_instance_id
            .as_deref()
            .is_none_or(|active| active == agent_instance_id);
        if is_active {
            if self.agent_panel.active_agent_instance_id.is_none() {
                self.agent_panel.active_agent_instance_id = Some(agent_instance_id.to_string());
            }
            apply(&mut self.timeline);
        } else {
            apply(
                self.agent_timelines
                    .entry(agent_instance_id.to_string())
                    .or_insert_with(crate::features::timeline::Timeline::new),
            );
        }
    }

    fn accepts_session(&self, session_id: &str) -> bool {
        !self.session.initializing && self.session.id.as_deref() == Some(session_id)
    }

    fn accepts_reconcile(&self, session_id: &str) -> bool {
        self.session.opening_id.as_deref().map_or_else(
            || self.session.id.as_deref() == Some(session_id),
            |target| target == session_id,
        )
    }

    fn select_agent_timeline(&mut self, agent_instance_id: &str) {
        if self.agent_panel.active_agent_instance_id.as_deref() == Some(agent_instance_id) {
            return;
        }
        if let Some(previous) = self
            .agent_panel
            .active_agent_instance_id
            .replace(agent_instance_id.to_string())
        {
            let previous_timeline = std::mem::replace(
                &mut self.timeline,
                self.agent_timelines
                    .remove(agent_instance_id)
                    .unwrap_or_else(crate::features::timeline::Timeline::new),
            );
            self.agent_timelines.insert(previous, previous_timeline);
        } else {
            self.timeline = self
                .agent_timelines
                .remove(agent_instance_id)
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
                let agent_instance_id = committed.agent_instance_id.clone();
                let mut consistent = true;
                self.with_agent_timeline(&agent_instance_id, |timeline| {
                    consistent = timeline.apply_committed(committed);
                });
                if !consistent && let Some(session_id) = self.session.id.clone() {
                    effects.push(Effect::send(Command::StateSnapshot {
                        command_id: command_id(),
                        session_id,
                    }));
                }
            }
            Event::StreamItem(patch) => {
                let Some(session_id) = patch.session_id.as_deref() else {
                    return effects;
                };
                if !self.accepts_session(session_id) {
                    return effects;
                }
                let Some(agent_instance_id) = patch.agent_instance_id.clone() else {
                    return effects;
                };
                self.with_agent_timeline(&agent_instance_id, |timeline| {
                    timeline.apply_stream_item(&patch);
                });
            }
            Event::SessionReconciled(reconciled) => {
                if self.accepts_reconcile(&reconciled.session_id) {
                    let selected_agent_instance_id =
                        reconciled.snapshot.selected_agent_instance_id.clone();
                    self.session.id = Some(reconciled.session_id.clone());
                    self.session.opening_id = None;
                    self.session.previous_live_id = None;
                    self.session.initializing = false;
                    self.apply_snapshot(reconciled.snapshot);
                    self.agent_panel.agents.clear();
                    for agent in reconciled.agents {
                        self.agent_panel
                            .upsert_agent(crate::features::agent_status::AgentEntry {
                                agent_id: agent.agent_id,
                                agent_instance_id: agent.agent_instance_id,
                                name: agent.name,
                                parent_agent_instance_id: agent.parent_agent_instance_id,
                                lifecycle: agent.lifecycle,
                                activity: agent.activity,
                                unread_report_count: agent.unread_report_count,
                                status: agent.status,
                            });
                    }
                    self.agent_panel.mark_hydrated();
                    let active_agent_instance_id = selected_agent_instance_id
                        .filter(|selected| {
                            self.agent_panel
                                .agents
                                .iter()
                                .any(|agent| &agent.agent_instance_id == selected)
                        })
                        .or_else(|| {
                            self.agent_panel
                                .agents
                                .iter()
                                .find(|agent| agent.parent_agent_instance_id.is_none())
                                .or_else(|| self.agent_panel.agents.first())
                                .map(|agent| agent.agent_instance_id.clone())
                        });
                    if let Some(active_agent_instance_id) = active_agent_instance_id {
                        self.select_agent_timeline(&active_agent_instance_id);
                    }
                    if let Some(name) = self.initial_options.session_name.take() {
                        effects.push(Effect::send(Command::SessionRename {
                            command_id: command_id(),
                            session_id: reconciled.session_id.clone(),
                            name,
                        }));
                    }
                    if let Some(text) = self.session.pending_turn_text.take() {
                        if let Some(target_agent_instance_id) =
                            self.agent_panel.active_agent_instance_id.clone()
                        {
                            let submit_command_id = command_id();
                            self.session.pending.track(
                                submit_command_id.clone(),
                                super::pending::PendingCommandKind::ChatSubmit,
                            );
                            effects.push(Effect::send(Command::ChatSubmit {
                                command_id: submit_command_id,
                                session_id: reconciled.session_id,
                                target_agent_instance_id,
                                text,
                            }));
                            self.status = "submitted first message".to_string();
                        } else {
                            self.session.pending_turn_text = Some(text);
                            self.status = "waiting for root agent".to_string();
                        }
                    }
                }
            }
            Event::SessionCleared(cleared) => {
                if self.session.id.as_deref() == Some(&cleared.previous_session_id) {
                    self.clear_session_view();
                    self.session
                        .pending
                        .clear_kind(super::pending::PendingCommandKind::SessionDelete);
                    self.session.pending.delete_session_id = None;
                    self.status = "session deleted".to_string();
                    self.notify(NotificationLevel::Warning, "session deleted");
                    self.clear_focus();
                }
            }
            Event::AgentChanged(agent) => {
                if !self.accepts_session(&agent.session_id) {
                    return effects;
                }
                self.agent_panel
                    .upsert_agent(crate::features::agent_status::AgentEntry {
                        agent_id: agent.agent_id,
                        agent_instance_id: agent.agent_instance_id,
                        name: agent.name,
                        parent_agent_instance_id: agent.parent_agent_instance_id,
                        lifecycle: agent.lifecycle,
                        activity: agent.activity,
                        unread_report_count: agent.unread_report_count,
                        status: agent.status,
                    });
            }
            Event::Interaction(piko_protocol::InteractionEvent::Requested {
                session_id,
                agent_instance_id,
                interaction_id,
                title,
                questions,
                require_confirm,
                auto_resolution_ms,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.interactions.push(
                    interaction_id,
                    agent_instance_id,
                    title,
                    questions,
                    require_confirm,
                );
                if auto_resolution_ms.is_none() {
                    self.push_focus(AppMode::ToolInteraction);
                }
            }
            Event::Interaction(piko_protocol::InteractionEvent::Resolved {
                session_id,
                interaction_id,
                status: _,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.interactions.resolve(&interaction_id);
                if self.interactions.is_empty()
                    && self.focus_manager.active_mode() == AppMode::ToolInteraction
                {
                    self.clear_focus();
                }
            }
            Event::TurnDiff(diff) => {
                if !self.accepts_session(&diff.session_id) {
                    return effects;
                }
                self.last_turn_id = Some(diff.turn_id.clone());
                self.last_turn_diff = Some(diff.clone());
                self.status = format!(
                    "turn {} changed {} file{}",
                    diff.turn_id,
                    diff.files.len(),
                    if diff.files.len() == 1 { "" } else { "s" }
                );
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::Empty),
                ..
            }
            | Event::CommandResponse { result: Err(_), .. } => {}
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::PromptDebugged { snapshot, .. }),
                ..
            } => {
                self.diagnostics.set_prompt_debug(&snapshot);
                self.push_focus(AppMode::Diagnostics);
                self.status = "prompt debug".to_string();
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::RolloutPaged { page, .. }),
                ..
            } => {
                self.diagnostics.set_rollout(&page);
                self.push_focus(AppMode::Diagnostics);
                self.status = format!("rollout {} item(s)", page.items.len());
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::TurnDiffGot { diff, .. }),
                ..
            } => match diff {
                Some(diff) => {
                    self.last_turn_id = Some(diff.turn_id.clone());
                    self.last_turn_diff = Some(diff.clone());
                    self.diagnostics.set_diff(&diff);
                    self.push_focus(AppMode::Diagnostics);
                    self.status = "turn diff".to_string();
                }
                None => {
                    self.diagnostics.set_message(
                        crate::features::diagnostics::DiagnosticsKind::Diff,
                        "turn diff",
                        "No diff recorded for that turn.",
                    );
                    self.push_focus(AppMode::Diagnostics);
                    self.status = "no turn diff".to_string();
                }
            },
            Event::CommandResponse {
                command_id: response_command_id,
                result:
                    Ok(piko_protocol::CommandResult::SessionCreated {
                        session_id, cwd, ..
                    }),
            } => {
                // Keep initializing until SessionReconciled hydrates the view (H5).
                let pending = self.session.pending.take(&response_command_id);
                if pending != Some(super::pending::PendingCommandKind::SessionCreate)
                    || self.session.opening_id.is_some()
                {
                    return effects;
                }
                self.session.opening_id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                let _ = cwd;
                self.notify(NotificationLevel::Info, "session created");
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
                command_id: response_command_id,
                result: Ok(piko_protocol::CommandResult::SessionOpened { session_id, .. }),
            } => {
                // Identity only — agents stay loading until SessionReconciled (H3/H5).
                let pending = self.session.pending.take(&response_command_id);
                let target_matches = self
                    .session
                    .opening_id
                    .as_deref()
                    .is_none_or(|target| target == session_id);
                if pending != Some(super::pending::PendingCommandKind::SessionOpen)
                    || !target_matches
                {
                    return effects;
                }
                self.session.opening_id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                self.notify(NotificationLevel::Info, "session opened");
                if self.focus_manager.active_mode() == AppMode::Sessions {
                    self.clear_focus();
                }
            }
            Event::CommandResponse {
                command_id: response_command_id,
                result: Ok(piko_protocol::CommandResult::SessionListed { sessions, .. }),
            } => {
                let _ = self.session.pending.take(&response_command_id);
                self.sessions.load(sessions);
                if self.session.continue_requested {
                    self.session.continue_requested = false;
                    if let Some(session_id) = self.sessions.selected_session_id() {
                        self.begin_session_hydration(Some(session_id.clone()));
                        let open_id = command_id();
                        self.session.pending.track(
                            open_id.clone(),
                            super::pending::PendingCommandKind::SessionOpen,
                        );
                        effects.push(Effect::send(Command::SessionOpen {
                            command_id: open_id,
                            session_id,
                            session_path: None,
                        }));
                        self.status = "opening latest session".to_string();
                    } else {
                        self.begin_session_hydration(None);
                        let create_id = command_id();
                        self.session.pending.track(
                            create_id.clone(),
                            super::pending::PendingCommandKind::SessionCreate,
                        );
                        effects.push(Effect::send(Command::SessionCreate {
                            command_id: create_id,
                            cwd: self.cwd.to_string_lossy().into_owned(),
                        }));
                        self.status = "no sessions found; creating session".to_string();
                    }
                    return effects;
                }
                self.push_focus(AppMode::Sessions);
                self.status = format!("{} sessions available", self.sessions.list.items.len());
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.session
                    .pending
                    .clear_kind(super::pending::PendingCommandKind::ChatSubmit);
                self.session.active_turns.insert(
                    agent_instance_id.clone(),
                    super::ActiveTurnUi {
                        turn_id: turn_id.clone(),
                        status: piko_protocol::TurnStatus::Queued,
                    },
                );
                self.status = format!("turn {turn_id} queued ({agent_instance_id})");
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Started {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.session
                    .pending
                    .clear_kind(super::pending::PendingCommandKind::ChatSubmit);
                self.session.active_turns.insert(
                    agent_instance_id.clone(),
                    super::ActiveTurnUi {
                        turn_id: turn_id.clone(),
                        status: piko_protocol::TurnStatus::Running,
                    },
                );
                self.status = format!("turn {turn_id} running ({agent_instance_id})");
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Completed {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                // Usage chrome is host-authoritative via Event::Usage only.
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} completed");
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Failed {
                session_id,
                turn_id,
                agent_instance_id,
                error,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} failed");
                self.push_error(error);
            }
            Event::TurnLifecycle(piko_protocol::TurnEvent::Cancelled {
                session_id,
                turn_id,
                agent_instance_id,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if self
                    .session
                    .active_turns
                    .get(&agent_instance_id)
                    .is_some_and(|active| active.turn_id == turn_id)
                {
                    self.session.active_turns.remove(&agent_instance_id);
                }
                self.last_turn_id = Some(turn_id.clone());
                self.status = format!("turn {turn_id} cancelled");
            }
            Event::AgentRunLifecycle(_) => {}
            Event::Approval(piko_protocol::ApprovalEvent::Requested {
                session_id,
                agent_instance_id,
                approval_id,
                tool_name,
                tool_args,
                prompt,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.approvals.push(PendingApproval {
                    id: approval_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    tool_name: tool_name.clone(),
                    args: tool_args,
                    prompt,
                });
                if let Some(agent) = self
                    .agent_panel
                    .agents
                    .iter_mut()
                    .find(|a| a.agent_instance_id == agent_instance_id)
                {
                    agent.activity = piko_protocol::AgentActivity::WaitingForApproval;
                }
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
                session_id,
                approval_id,
                decision,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                let agent_instance_id = self
                    .approvals
                    .pending
                    .iter()
                    .find(|a| a.id == approval_id)
                    .map(|a| a.agent_instance_id.clone());
                self.approvals.resolve(&approval_id);
                if let Some(agent_id) = agent_instance_id {
                    let still_blocked = self
                        .approvals
                        .pending
                        .iter()
                        .any(|a| a.agent_instance_id == agent_id);
                    if !still_blocked
                        && let Some(agent) = self
                            .agent_panel
                            .agents
                            .iter_mut()
                            .find(|a| a.agent_instance_id == agent_id)
                    {
                        agent.activity = if self.session.active_turns.contains_key(&agent_id) {
                            piko_protocol::AgentActivity::Running
                        } else {
                            piko_protocol::AgentActivity::Idle
                        };
                    }
                }
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
            Event::Queue(piko_protocol::QueueEvent::Updated {
                session_id,
                steer_count,
                follow_up_count,
                next_turn_count,
                steer_preview,
                follow_up_preview,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
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
                command_id,
                result: Ok(piko_protocol::CommandResult::ModelListed { providers, .. }),
            } => {
                let pending = self.session.pending.take(&command_id);
                self.model.providers = providers.clone();
                let provider_names: Vec<String> =
                    providers.iter().map(|p| p.provider.clone()).collect();
                self.auth_selector.reset(&provider_names);
                self.models.load(flatten_models(providers));
                match pending {
                    Some(super::pending::PendingCommandKind::BootstrapModels) => {
                        // Silent catalog warm-up for context size chrome.
                        self.status = format!("{} models cached", self.models.len());
                    }
                    Some(super::pending::PendingCommandKind::ModelList) => {
                        // Interactive open already pushed Models focus.
                        self.status = format!("{} models available", self.models.len());
                    }
                    _ => {
                        // Untracked ModelList (e.g. /login provider probe).
                        if self.mode != AppMode::AuthSelector
                            && !matches!(self.mode, AppMode::Models)
                        {
                            self.push_focus(AppMode::Models);
                        }
                        self.status = format!("{} models available", self.models.len());
                    }
                }
            }
            Event::CommandResponse {
                command_id,
                result: Ok(piko_protocol::CommandResult::CommandCatalogListed { commands, .. }),
            } => {
                self.command_catalog = super::command::merge_command_catalog(&commands);
                self.refresh_suggestions();
                self.finish_bootstrap_command(&command_id);
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::ProcessListed { processes, .. }),
                ..
            } => {
                if processes.is_empty() {
                    self.notify(NotificationLevel::Info, "no processes running");
                    self.status = "no processes running".to_string();
                } else {
                    let lines: Vec<String> = processes
                        .iter()
                        .map(|p| {
                            let state = if p.exited {
                                p.exit_code
                                    .map(|code| format!(" exit={code}"))
                                    .unwrap_or_else(|| {
                                        p.signal
                                            .map(|sig| format!(" signal={sig}"))
                                            .unwrap_or_else(|| " exited".to_string())
                                    })
                            } else {
                                String::new()
                            };
                            format!(
                                "{}  pid {}  {}{}  ({})",
                                p.process_id, p.pid, p.command, state, p.cwd
                            )
                        })
                        .collect();
                    self.notify(NotificationLevel::Info, lines.join("\n"));
                    self.status = format!("{} process(es) running", processes.len());
                }
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::ProcessStopped {
                        process_id,
                        stopped,
                        exit_code,
                        signal,
                        ..
                    }),
                ..
            } => {
                if stopped {
                    let detail = exit_code
                        .map(|code| format!(" (exit {code})"))
                        .or_else(|| signal.map(|sig| format!(" (signal {sig})")))
                        .unwrap_or_default();
                    self.notify(
                        NotificationLevel::Info,
                        format!("stopped {process_id}{detail}"),
                    );
                    self.status = format!("stopped {process_id}");
                } else {
                    self.notify(
                        NotificationLevel::Warning,
                        format!("no such process: {process_id}"),
                    );
                    self.status = format!("no such process: {process_id}");
                }
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::McpStatusListed { servers, .. }),
                ..
            } => {
                self.mcp.set_servers(servers);
                self.push_focus(AppMode::Mcp);
                let connected = self.mcp.connected_count();
                let names: Vec<String> = self
                    .mcp
                    .servers()
                    .iter()
                    .filter(|s| s.connected)
                    .map(|s| s.name.clone())
                    .collect();
                self.status = format!("{connected} MCP server(s) connected");
                self.notify(
                    NotificationLevel::Info,
                    if names.is_empty() {
                        "no MCP servers connected".to_string()
                    } else {
                        format!("MCP servers: {}", names.join(", "))
                    },
                );
            }
            Event::CommandResponse {
                result: Ok(piko_protocol::CommandResult::AgentSpecListed { agents, .. }),
                ..
            } => {
                self.agents.load(agents);
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::AgentListed {
                        session_id, agents, ..
                    }),
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.agent_panel.agents.clear();
                for a in &agents {
                    self.agent_panel
                        .upsert_agent(crate::features::agent_status::AgentEntry {
                            agent_id: a.agent_id.clone(),
                            agent_instance_id: a.agent_instance_id.clone(),
                            name: a.name.clone(),
                            parent_agent_instance_id: a.parent_agent_instance_id.clone(),
                            lifecycle: a.lifecycle,
                            activity: a.activity.clone(),
                            unread_report_count: a.unread_report_count,
                            status: a.status.clone(),
                        });
                }
                self.agent_panel.mark_hydrated();
                self.status = format!("{} agents active", agents.len());
            }
            Event::CommandResponse {
                result:
                    Ok(piko_protocol::CommandResult::AgentSubscribed {
                        session_id,
                        agent_instance_id,
                        agent_id,
                        snapshot,
                        replay,
                        ..
                    }),
                ..
            } => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                // Always replace the visible timeline from host authority. Focus may
                // have already marked this agent active without swapping/clearing,
                // so select + clear before applying replay.
                self.select_agent_timeline(&agent_instance_id);
                self.timeline.clear();
                let events = if snapshot.events.is_empty() {
                    replay
                } else {
                    snapshot.events
                };
                for event in events {
                    effects.extend(self.apply_event(*event.message));
                }
                self.status = format!("subscribed to agent {agent_id} ({agent_instance_id})");
            }
            Event::Model(piko_protocol::ModelEvent::ConfigChanged {
                model_id,
                provider,
                thinking_level,
                context_window,
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
                self.model.host_context_window = context_window.filter(|w| *w > 0);
                if self.model.active_model_id.is_some() && self.model.active_provider.is_some() {
                    self.status = format!("model {provider}/{model_id}");
                } else {
                    self.status = "no model active".to_string();
                }
            }
            Event::Usage(piko_protocol::UsageEvent::Updated {
                session_id,
                used,
                size,
                cumulative,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                if used > 0 {
                    self.session.last_context_tokens = Some(used);
                }
                if let Some(window) = size.filter(|w| *w > 0) {
                    self.model.host_context_window = Some(window);
                }
                if let Some(cumulative) = cumulative {
                    // Host ledger is authoritative when present.
                    self.session.cumulative_usage = Some(cumulative);
                }
            }
            Event::CommandResponse {
                command_id,
                result: Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }),
            } => {
                if namespace == "tui" {
                    self.tui_config = TuiConfig::from_hostd_settings(Some(&value));
                    self.editor.configure(&self.tui_config.editor);
                    self.timeline.thinking_visible = !self.tui_config.hide_thinking_block;
                }
                self.finish_bootstrap_command(&command_id);
            }
        }
        effects
    }

    // ── snapshot application ──────────────────────────────────────────────────

    fn apply_snapshot(&mut self, snapshot: SessionSnapshot) {
        self.timeline.clear();
        self.agent_timelines.clear();
        self.agent_panel.active_agent_instance_id = None;
        self.queue_status = QueueStatus::default();
        // Authoritative session ledger replaces any local roll-up.
        self.session.cumulative_usage = snapshot.cumulative_usage.clone();
        self.session.last_context_tokens = snapshot
            .active_turns
            .iter()
            .rev()
            .find_map(|turn| {
                turn.usage
                    .as_ref()
                    .map(crate::features::bottom_bar::context_tokens_from_usage)
            })
            .filter(|&tokens| tokens > 0)
            .or_else(|| {
                last_context_tokens_from_entries(
                    &snapshot.entries,
                    snapshot.current_leaf_id.as_deref(),
                )
            });
        self.tree
            .load(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        let active_entries =
            get_active_branch_entries(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        for entry in active_entries {
            match entry {
                SessionTreeEntry::Message(message_entry) => {
                    let agent_instance_id = message_entry.agent_instance_id.clone();
                    let committed = piko_protocol::TranscriptCommittedEvent {
                        session_id: snapshot.session_id.clone(),
                        agent_instance_id: agent_instance_id.clone(),
                        agent_id: message_entry.agent_id,
                        source_turn_id: message_entry.source_turn_id,
                        message_id: message_entry.id,
                        transcript_seq: message_entry.transcript_seq,
                        message: message_entry.message,
                    };
                    self.with_agent_timeline(&agent_instance_id, |timeline| {
                        timeline.apply_committed(committed);
                    });
                }
                SessionTreeEntry::ToolCall(tool_call) => {
                    let agent_instance_id = tool_call.agent_instance_id.clone();
                    let tool = ToolEntry::new(
                        tool_call.tool_call_id,
                        tool_call.tool_name,
                        ToolStatus::Running,
                        compact_json(&tool_call.arguments),
                        None,
                        tool_call.parent_message_id,
                    );
                    if let Some(agent_instance_id) = agent_instance_id {
                        self.with_agent_timeline(&agent_instance_id, |timeline| {
                            if !timeline.upsert_tool(tool.clone()) {
                                timeline.push(TimelineEntry::Tool(tool));
                            }
                        });
                    } else if !self.timeline.upsert_tool(tool.clone()) {
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

        self.session.active_turns = snapshot
            .active_turns
            .into_iter()
            .map(|turn| {
                (
                    turn.agent_instance_id,
                    super::ActiveTurnUi {
                        turn_id: turn.turn_id,
                        status: turn.status,
                    },
                )
            })
            .collect();

        self.approvals.clear();
        for approval in snapshot.pending_approvals {
            let tool_name = if approval.tool_name.is_empty() {
                approval
                    .request
                    .get("toolName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string()
            } else {
                approval.tool_name
            };
            self.approvals.push(PendingApproval {
                id: approval.approval_id,
                agent_instance_id: approval.agent_instance_id,
                tool_name,
                args: approval.request,
                prompt: approval.prompt,
            });
        }
        self.interactions.clear();
        for interaction in snapshot.pending_interactions {
            self.interactions.push(
                interaction.interaction_id,
                interaction.agent_instance_id,
                interaction.title,
                interaction.questions,
                interaction.require_confirm,
            );
        }
        if !self.approvals.is_empty() && self.focus_manager.active_mode() != AppMode::Approval {
            self.push_focus(AppMode::Approval);
        } else if !self.interactions.is_empty()
            && self.focus_manager.active_mode() != AppMode::ToolInteraction
        {
            self.push_focus(AppMode::ToolInteraction);
        }
    }
}

/// Walk the active branch newest-first for the latest assistant prompt-side tokens.
fn last_context_tokens_from_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Option<u64> {
    let branch = get_active_branch_entries(entries, current_leaf_id);
    for entry in branch.into_iter().rev() {
        if let SessionTreeEntry::Message(message_entry) = entry
            && let piko_protocol::Message::Assistant {
                usage: Some(usage), ..
            } = message_entry.message
        {
            let tokens = crate::features::bottom_bar::context_tokens_from_usage(&usage);
            if tokens > 0 {
                return Some(tokens);
            }
        }
    }
    None
}
