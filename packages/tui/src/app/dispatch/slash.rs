use super::*;

impl AppState {
    pub(super) fn dispatch_slash_action(&mut self, action: SlashAction) -> Vec<Effect> {
        let mut effects = Vec::new();
        match action {
            SlashAction::New => {
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
                self.clear_focus();
                self.status = "creating session".to_string();
            }
            SlashAction::Fork(entry_id) => effects.extend(self.fork_session(entry_id)),
            SlashAction::Clone => effects.extend(self.fork_session(None)),
            SlashAction::Rename(name) => effects.extend(self.rename_session(name)),
            SlashAction::Import(path) => {
                self.begin_session_hydration(None);
                let import_id = command_id();
                self.session.pending.track(
                    import_id.clone(),
                    crate::app::pending::PendingCommandKind::SessionOpen,
                );
                effects.push(Effect::send(Command::SessionImport {
                    command_id: import_id,
                    path,
                }));
                self.status = "importing session".to_string();
            }
            SlashAction::Delete => effects.extend(self.delete_current_session()),
            SlashAction::Login(provider_opt) => {
                if let Some(provider) = provider_opt {
                    effects.push(Effect::send(Command::AuthLoginOAuth {
                        command_id: command_id(),
                        provider,
                    }));
                    self.status = "starting OAuth login".to_string();
                } else {
                    effects.push(Effect::send(Command::ModelList {
                        command_id: command_id(),
                    }));
                    let provider_names: Vec<String> = self
                        .model
                        .providers
                        .iter()
                        .map(|p| p.provider.clone())
                        .collect();
                    let auth_names: Vec<String> = self
                        .model
                        .providers
                        .iter()
                        .filter(|p| p.has_auth)
                        .map(|p| p.provider.clone())
                        .collect();
                    self.auth_selector.reset(&provider_names, &auth_names);
                    self.push_surface(SurfaceId::AuthSelector);
                    self.status = "Select authentication method".to_string();
                }
            }
            SlashAction::Logout(provider_opt) => {
                let Some(provider) = provider_opt.or_else(|| self.model.active_provider.clone())
                else {
                    self.status = "usage: /logout <provider>".to_string();
                    return effects;
                };
                effects.push(Effect::send(Command::AuthLogout {
                    command_id: command_id(),
                    provider: provider.clone(),
                }));
                self.clear_focus();
                self.status = format!("logging out {provider}");
            }
            SlashAction::Compact => {
                let Some(session_id) = self.session.id.clone() else {
                    self.status = "no active session to compact".to_string();
                    return effects;
                };
                let Some(agent_instance_id) = self.agent_panel.active_agent_instance_id.clone()
                else {
                    self.status = "no active agent to compact".to_string();
                    return effects;
                };
                effects.push(Effect::send(Command::SessionCompact {
                    command_id: command_id(),
                    session_id,
                    agent_instance_id,
                    mode: piko_protocol::command::CompactMode::Summarize,
                }));
                self.clear_focus();
                self.status = "compaction requested".to_string();
            }
            SlashAction::ListProcesses => {
                effects.push(Effect::send(Command::ProcessList {
                    command_id: command_id(),
                }));
                self.clear_focus();
                self.status = "listing processes".to_string();
            }
            SlashAction::ListMcpStatus => {
                effects.push(Effect::send(Command::McpStatus {
                    command_id: command_id(),
                }));
                self.clear_focus();
                self.status = "listing mcp servers".to_string();
            }
            SlashAction::RequestDiff => effects.extend(self.request_turn_diff()),
            SlashAction::RequestPromptDebug => effects.extend(self.request_prompt_debug()),
        }
        effects
    }

    // ── surface selection helpers ─────────────────────────────────────────────
}
