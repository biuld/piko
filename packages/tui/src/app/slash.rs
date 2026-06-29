use crate::{action::Action, app::AppState, host::HostdClient};

impl AppState {
    // ── slash command parsing ─────────────────────────────────────────────────

    pub fn try_slash_command(&mut self, host: &mut HostdClient, text: &str) -> bool {
        let mut parts = text.split_whitespace();
        let Some(command) = parts.next() else {
            return false;
        };
        match command {
            "/help" | "/?" => {
                self.dispatch(host, Action::OpenHelp);
                true
            }
            "/commands" | "/command" => {
                self.dispatch(host, Action::OpenCommands);
                true
            }
            "/sessions" | "/session" | "/resume" => {
                self.dispatch(host, Action::RequestSessions);
                true
            }
            "/tree" | "/branches" => {
                self.dispatch(host, Action::OpenTree);
                true
            }
            "/models" | "/model" => {
                self.dispatch(host, Action::RequestModels);
                true
            }
            "/settings" | "/config" => {
                self.dispatch(host, Action::OpenSettings);
                true
            }
            "/status" => {
                self.dispatch(host, Action::OpenStatus);
                true
            }
            "/new" => {
                self.dispatch(host, Action::SlashNew);
                true
            }
            "/fork" => {
                let entry_id = parts
                    .next()
                    .map(ToString::to_string)
                    .or_else(|| self.tree.selected_entry_id());
                self.dispatch(host, Action::SlashFork(entry_id));
                true
            }
            "/clone" => {
                self.dispatch(host, Action::SlashClone);
                true
            }
            "/name" | "/rename" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.status = "usage: /name <session name>".to_string();
                } else {
                    self.dispatch(host, Action::SlashRename(name));
                }
                true
            }
            "/import" => {
                let path = parts.collect::<Vec<_>>().join(" ");
                if path.is_empty() {
                    self.status = "usage: /import <jsonl path>".to_string();
                } else {
                    self.dispatch(host, Action::SlashImport(path));
                }
                true
            }
            "/export" => {
                if let Some(ref session_id) = self.session_id {
                    self.status = format!(
                        "Session saved in ~/.piko/agent/sessions/ under ID: {}",
                        session_id
                    );
                } else {
                    self.status = "No active session".to_string();
                }
                true
            }
            "/delete" => {
                if parts.next() == Some("confirm") {
                    self.dispatch(host, Action::SlashDelete);
                } else {
                    self.status = "usage: /delete confirm".to_string();
                }
                true
            }
            "/login" => {
                let provider = parts.next().unwrap_or("anthropic").to_string();
                self.dispatch(host, Action::SlashLogin(provider));
                true
            }
            "/logout" => {
                let provider = parts.next().unwrap_or("anthropic").to_string();
                self.dispatch(host, Action::SlashLogout(provider));
                true
            }
            "/compact" => {
                self.dispatch(host, Action::SlashCompact);
                true
            }
            _ => false,
        }
    }
}
