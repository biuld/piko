use super::*;

impl AppState {
    pub(super) fn apply_command_response(
        &mut self,
        response_command_id: String,
        result: Result<piko_protocol::CommandResult, String>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        match result {
            Ok(piko_protocol::CommandResult::Empty) | Err(_) => {}
            Ok(piko_protocol::CommandResult::PromptDebugged { snapshot, .. }) => {
                self.diagnostics.set_prompt_debug(&snapshot);
                self.push_focus(AppMode::Diagnostics);
                self.status = "prompt debug".to_string();
            }
            Ok(piko_protocol::CommandResult::RolloutPaged { page, .. }) => {
                self.diagnostics.set_rollout(&page);
                self.push_focus(AppMode::Diagnostics);
                self.status = format!("rollout {} item(s)", page.items.len());
            }
            Ok(piko_protocol::CommandResult::TurnDiffGot { diff, .. }) => match diff {
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
                if self.focus_manager.active_mode() == AppMode::Sessions {
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
                self.push_focus(AppMode::Sessions);
                self.status = format!("{} sessions available", self.sessions.list.items.len());
            }
            Ok(piko_protocol::CommandResult::ModelListed { providers, .. }) => {
                let pending = self.session.pending.take(&response_command_id);
                self.model.providers = providers.clone();
                let provider_names: Vec<String> =
                    providers.iter().map(|p| p.provider.clone()).collect();
                self.auth_selector.reset(&provider_names);
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
                        if self.mode != AppMode::AuthSelector
                            && !matches!(self.mode, AppMode::Models)
                        {
                            self.push_focus(AppMode::Models);
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
            Ok(piko_protocol::CommandResult::ProcessStopped {
                process_id,
                stopped,
                exit_code,
                signal,
                ..
            }) => {
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
            Ok(piko_protocol::CommandResult::McpStatusListed { servers, .. }) => {
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
            Ok(piko_protocol::CommandResult::AgentSpecListed { agents, .. }) => {
                self.agents.load(agents);
            }
            Ok(piko_protocol::CommandResult::AgentListed {
                session_id, agents, ..
            }) => {
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
            Ok(piko_protocol::CommandResult::ConfigEntry { namespace, value }) => {
                if namespace == "tui" {
                    self.tui_config = TuiConfig::from_hostd_settings(Some(&value));
                    self.editor.configure(&self.tui_config.editor);
                    self.timeline.thinking_visible = !self.tui_config.hide_thinking_block;
                    if let Some(name) = value
                        .get("theme")
                        .and_then(|t| t.get("name"))
                        .and_then(|n| n.as_str())
                    {
                        self.theme = crate::theme::Theme::load(name);
                    }
                } else if namespace == "host" {
                    self.host_settings.apply_host_json(&value);
                    if let Some(level) = self.host_settings.thinking_level.clone()
                        && self.model.active_thinking_level.is_none()
                    {
                        self.model.active_thinking_level = Some(level);
                    }
                    // Refresh open Settings so ValueSummaries track host authority.
                    if self.mode == AppMode::Settings {
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
