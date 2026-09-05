use super::*;

impl AppState {
    pub(super) fn apply_command_response(
        &mut self,
        response_command_id: String,
        result: Result<piko_protocol::CommandResult, String>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let pending_history =
            self.history.pending_command_id.as_deref() == Some(&response_command_id);
        let is_history = matches!(
            &result,
            Ok(
                piko_protocol::CommandResult::SessionHistoryOverviewGot { .. }
                    | piko_protocol::CommandResult::SessionHistoryWorkPaged { .. }
                    | piko_protocol::CommandResult::SessionHistoryJournalPaged { .. }
                    | piko_protocol::CommandResult::SessionHistoryTranscriptPaged { .. }
                    | piko_protocol::CommandResult::SessionHistoryItemGot { .. }
                    | piko_protocol::CommandResult::HistoryRevisionChanged { .. }
            )
        );
        if (is_history || response_command_id.starts_with("history:")) && !pending_history {
            return effects;
        }
        if pending_history {
            self.history.pending_command_id = None;
            self.history.loading = false;
            if let Err(error) = &result {
                if self.history.detail_loading {
                    self.history.detail_loading = false;
                    self.history.detail_error = Some(error.clone());
                } else {
                    self.history.error = Some(error.clone());
                }
                return effects;
            }
            if let Ok(piko_protocol::CommandResult::SessionListed { sessions, .. }) = result {
                self.history.sessions = sessions;
                self.history.selected = 0;
                return effects;
            }
        }
        match result {
            Ok(piko_protocol::CommandResult::HistoryRevisionChanged { session_id, .. }) => {
                return self.open_history(Some(session_id));
            }
            Ok(piko_protocol::CommandResult::Empty) => {}
            Ok(piko_protocol::CommandResult::AgentInputSubmitted { receipt, .. }) => {
                self.session
                    .pending
                    .clear_kind(crate::app::pending::PendingCommandKind::AgentInputSubmit);
                self.session
                    .pending_submissions
                    .remove(&response_command_id);
                self.status = match receipt.disposition {
                    piko_protocol::AgentInputDisposition::PendingFollowUp => {
                        format!("input {} queued", receipt.input_id)
                    }
                    piko_protocol::AgentInputDisposition::PendingSteer => {
                        format!("input {} will steer", receipt.input_id)
                    }
                    _ => format!("input {} accepted", receipt.input_id),
                };
            }
            Ok(piko_protocol::CommandResult::AgentInputCancelled { receipt, .. }) => {
                self.status = if receipt.accepted {
                    format!("input {} cancelled", receipt.input_id)
                } else {
                    format!("input {} is no longer pending", receipt.input_id)
                };
            }
            Ok(piko_protocol::CommandResult::AgentInterrupted {
                agent_instance_id,
                accepted,
                ..
            }) => {
                self.status = if accepted {
                    format!("interrupt requested for {agent_instance_id}")
                } else {
                    format!("agent {agent_instance_id} is already idle")
                };
            }
            Ok(piko_protocol::CommandResult::AuthLoginStarted { provider, mode, .. }) => {
                self.status = format!("starting {provider} {mode:?} login");
            }
            Err(error) => {
                self.status = error.clone();
                self.notify(NotificationLevel::Error, error);
            }
            // Rollout is no longer surfaced by the TUI; ignore stale responses.
            Ok(piko_protocol::CommandResult::RolloutPaged { .. }) => {}
            Ok(piko_protocol::CommandResult::AgentWorkDiffGot { diff, .. }) => match diff {
                Some(diff) => {
                    self.last_root_input_id = Some(diff.root_input_id.clone());
                    self.last_agent_work_diff = Some(diff.clone());
                    self.diagnostics.set_diff(&diff);
                    self.push_surface(SurfaceId::Diagnostics);
                    self.status = "work diff".to_string();
                }
                None => {
                    self.diagnostics.set_message(
                        crate::features::diagnostics::DiagnosticsKind::Diff,
                        "work diff",
                        "No diff recorded for that input.",
                    );
                    self.push_surface(SurfaceId::Diagnostics);
                    self.status = "no work diff".to_string();
                }
            },
            Ok(piko_protocol::CommandResult::SessionHistoryOverviewGot { overview, .. }) => {
                let count = overview.works.len();
                self.history.set_overview(overview);
                self.status = format!("{count} historical work item(s)");
            }
            Ok(piko_protocol::CommandResult::SessionHistoryWorkPaged { page, .. }) => {
                let count = page.items.len();
                self.history.set_work(page);
                self.status = format!("{count} history item(s)");
            }
            Ok(piko_protocol::CommandResult::SessionHistoryJournalPaged { page, .. }) => {
                let count = page.commits.len();
                self.history.set_journal(page);
                self.status = format!("{count} journal commit(s)");
            }
            Ok(piko_protocol::CommandResult::SessionHistoryTranscriptPaged { page, .. }) => {
                let count = page.items.len();
                self.history.set_transcript(page);
                self.status = format!("{count} transcript item(s)");
            }
            Ok(piko_protocol::CommandResult::SessionHistoryItemGot { detail, .. }) => {
                self.history.set_detail(detail);
                self.status = "history detail".into();
            }
            Ok(piko_protocol::CommandResult::SessionCreated {
                session_id, cwd, ..
            }) => {
                // Keep initializing until SessionReconciled hydrates the view (H5).
                let pending = self.session.pending.take(&response_command_id);
                if pending != Some(crate::app::pending::PendingCommandKind::SessionCreate)
                    || self.session.opening_id.is_some()
                {
                    return effects;
                }
                self.session.opening_id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                let _ = cwd;
                self.notify(NotificationLevel::Info, "session created");
            }
            Ok(piko_protocol::CommandResult::SessionNavigated { editor_text, .. }) => {
                if let Some(text) = editor_text
                    && self.editor.is_empty()
                {
                    self.editor.insert_paste(&text, &self.tui_config.editor);
                }
            }
            Ok(piko_protocol::CommandResult::SessionOpened { session_id, .. }) => {
                // Identity only — agents stay loading until SessionReconciled (H3/H5).
                let pending = self.session.pending.take(&response_command_id);
                let target_matches = self
                    .session
                    .opening_id
                    .as_deref()
                    .is_none_or(|target| target == session_id);
                if pending != Some(crate::app::pending::PendingCommandKind::SessionOpen)
                    || !target_matches
                {
                    return effects;
                }
                self.session.opening_id = Some(session_id.clone());
                self.status = format!("session {session_id}");
                self.notify(NotificationLevel::Info, "session opened");
                if self.focus_manager.active_mode() == AppMode::Surface(SurfaceId::Sessions) {
                    self.clear_focus();
                }
            }
            Ok(piko_protocol::CommandResult::SessionListed { sessions, .. }) => {
                let _ = self.session.pending.take(&response_command_id);
                self.sessions.load(sessions);
                if self.session.continue_requested {
                    self.session.continue_requested = false;
                    if let Some(session_id) = self.sessions.selected_session_id() {
                        self.begin_session_hydration(Some(session_id.clone()));
                        let open_id = command_id();
                        self.session.pending.track(
                            open_id.clone(),
                            crate::app::pending::PendingCommandKind::SessionOpen,
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
                            crate::app::pending::PendingCommandKind::SessionCreate,
                        );
                        effects.push(Effect::send(Command::SessionCreate {
                            command_id: create_id,
                            cwd: self.cwd.to_string_lossy().into_owned(),
                        }));
                        self.status = "no sessions found; creating session".to_string();
                    }
                    return effects;
                }
                self.push_surface(SurfaceId::Sessions);
                self.status = format!("{} sessions available", self.sessions.list.items.len());
            }
            Ok(piko_protocol::CommandResult::ModelListed { providers, .. }) => {
                let pending = self.session.pending.take(&response_command_id);
                self.model.providers = providers.clone();
                let auth_names: Vec<String> = providers
                    .iter()
                    .filter(|p| p.has_auth)
                    .map(|p| p.provider.clone())
                    .collect();
                self.auth_selector.reset(&providers, &auth_names);
                self.models.load(flatten_models(providers));
                match pending {
                    Some(crate::app::pending::PendingCommandKind::BootstrapModels) => {
                        // Silent catalog warm-up for context size chrome.
                        self.status = format!("{} models cached", self.models.len());
                    }
                    Some(crate::app::pending::PendingCommandKind::ModelList) => {
                        // Interactive open already pushed Models focus.
                        self.status = format!("{} models available", self.models.len());
                    }
                    _ => {
                        // Untracked ModelList (e.g. /login provider probe).
                        if self.mode() != AppMode::Surface(SurfaceId::AuthSelector)
                            && !matches!(self.mode(), AppMode::Surface(SurfaceId::Models))
                        {
                            self.push_surface(SurfaceId::Models);
                        }
                        self.status = format!("{} models available", self.models.len());
                    }
                }
            }
            Ok(piko_protocol::CommandResult::CommandCatalogListed { commands, .. }) => {
                self.command_catalog = crate::app::command::merge_command_catalog(&commands);
                self.refresh_suggestions();
                self.finish_bootstrap_command(&response_command_id);
            }
            Ok(piko_protocol::CommandResult::ProcessListed { processes, .. }) => {
                let count = processes.len();
                self.processes.set_processes(processes);
                self.push_surface(SurfaceId::Processes);
                self.status = if count == 0 {
                    "no processes running".to_string()
                } else {
                    format!("{count} process(es) running")
                };
            }
            Ok(piko_protocol::CommandResult::ProcessStopped {
                process_id,
                stopped,
                exit_code,
                signal,
                ..
            }) => {
                if stopped {
                    self.processes.remove(&process_id);
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
            Ok(piko_protocol::CommandResult::McpStatusListed { servers, .. }) => {
                self.mcp.set_servers(servers);
                self.push_surface(SurfaceId::Mcp);
                let connected = self.mcp.connected_count();
                self.status = format!("{connected} MCP server(s) connected");
            }
            Ok(piko_protocol::CommandResult::AgentSpecListed { agents, .. }) => {
                self.status = format!("{} agent specs available", agents.len());
            }
            Ok(piko_protocol::CommandResult::AgentListed {
                session_id, agents, ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                self.agent_panel.list.clear();
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
            Ok(piko_protocol::CommandResult::AgentSubscribed {
                session_id,
                agent_instance_id,
                agent_id,
                snapshot,
                replay,
                ..
            }) => {
                if !self.accepts_session(&session_id) {
                    return effects;
                }
                // Always replace the visible timeline from host authority. Focus may
                // have already marked this agent active without swapping/clearing,
                // so select + clear before applying replay.
                self.select_agent_timeline(&agent_instance_id);
                self.timeline_mut().clear();
                let session_entries = self.timelines.session_entries().to_vec();
                for (entry, order) in session_entries {
                    let _ = self.timeline_mut().apply_session_entry(entry, order);
                }
                let model_steps = self.timelines.model_step_boundaries_for(&agent_instance_id);
                for boundary in model_steps {
                    let _ = self.timeline_mut().apply_model_step_committed(boundary);
                }
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
            Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }) => {
                if namespace == "tui" {
                    match TuiConfig::try_from_hostd_settings(Some(&value)) {
                        Ok(config) => {
                            let profile = self.binding_registry.profile().clone();
                            match crate::input::binding::BindingRegistry::compile(
                                profile,
                                Some(&config.keybindings),
                            ) {
                                Ok(registry) => {
                                    self.tui_config = config;
                                    self.binding_registry = registry;
                                    self.editor.configure(&self.tui_config.editor);
                                    self.timelines
                                        .set_thinking_visible(!self.tui_config.hide_thinking_block);
                                    self.tree.filter_mode = self.tui_config.tree.filter_mode.into();
                                    if let Some(name) = value
                                        .get("theme")
                                        .and_then(|t| t.get("name"))
                                        .and_then(|n| n.as_str())
                                    {
                                        self.theme = crate::theme::Theme::load(name)
                                            .for_color_level(self.binding_registry.profile().color);
                                    }
                                }
                                Err(diagnostics) => {
                                    let message = diagnostics
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                        .join("; ");
                                    self.notify(
                                        NotificationLevel::Warning,
                                        format!(
                                            "invalid keybindings; previous bindings retained: {message}"
                                        ),
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            self.notify(
                                NotificationLevel::Warning,
                                format!(
                                    "invalid TUI settings; previous settings retained: {error}"
                                ),
                            );
                        }
                    }
                } else if namespace == "host" {
                    self.host_settings.apply_host_json(&value);
                    if let Some(level) = self.host_settings.thinking_level.clone()
                        && self.model.active_thinking_level.is_none()
                    {
                        self.model.active_thinking_level = Some(level);
                    }
                    // Refresh open Settings so ValueSummaries track host authority.
                    if self.mode() == AppMode::Surface(SurfaceId::Settings) {
                        let snap = self.settings_snapshot();
                        // Rebuild without collapsing depth would need path restore;
                        // open-time refresh rebuilds from root with fresh summaries.
                        if self.settings.stack.at_root() {
                            self.settings.open_root(&snap);
                        }
                    }
                }
                self.finish_bootstrap_command(&response_command_id);
            }
        }
        effects
    }
}
