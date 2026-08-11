use crate::app::{
    AppState,
    command::{
        CommandTarget, HostCommandArgs, SlashAction, action_for_host_command,
        action_for_local_command,
    },
    effect::Effect,
};

impl AppState {
    // ── slash command parsing ─────────────────────────────────────────────────

    pub fn try_slash_command(&mut self, text: &str) -> Option<Vec<Effect>> {
        let mut parts = text.split_whitespace();
        let command = parts.next()?;
        if command == "/clear" {
            return Some(self.dispatch(SlashAction::New.into()));
        }
        let target = self
            .command_catalog
            .iter()
            .find(|entry| entry.slash == command)
            .map(|entry| entry.target.clone())?;

        let effects = match target {
            CommandTarget::Local(id) => self.dispatch(action_for_local_command(id)),
            CommandTarget::Host(id) => self.run_host_slash(&id, parts, text),
        };
        Some(effects)
    }

    /// Host ids with bespoke argument parsing from slash text. Ids not
    /// listed here fall through to the generic `action_for_host_command`
    /// mapping with no extra text arguments.
    fn run_host_slash<'a>(
        &mut self,
        id: &str,
        mut parts: impl Iterator<Item = &'a str>,
        submitted_text: &str,
    ) -> Vec<Effect> {
        match id {
            "session.fork" => {
                if let Some(entry_id) = parts.next().map(ToString::to_string) {
                    self.dispatch_host_command(
                        id,
                        HostCommandArgs {
                            fork_entry_id: Some(entry_id),
                            provider: None,
                        },
                    )
                } else {
                    self.tree_fork_mode = true;
                    self.tree.rebuild_visible_for_filter();
                    self.push_surface(crate::app::SurfaceId::Tree);
                    self.status = "Select a session tree entry to fork".to_string();
                    Vec::new()
                }
            }
            "session.rename" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.reject_slash(submitted_text, "usage: /rename <session name>")
                } else {
                    self.dispatch(SlashAction::Rename(name).into())
                }
            }
            "session.import" => {
                let path = parts.collect::<Vec<_>>().join(" ");
                if path.is_empty() {
                    self.reject_slash(submitted_text, "usage: /import <jsonl path>")
                } else {
                    self.dispatch(SlashAction::Import(path).into())
                }
            }
            "session.delete" => {
                if parts.next() == Some("confirm") {
                    self.dispatch(SlashAction::Delete.into())
                } else {
                    self.reject_slash(submitted_text, "usage: /delete confirm")
                }
            }
            "auth.login" => {
                let provider = parts.next().map(|s| s.to_string());
                self.dispatch_host_command(
                    id,
                    HostCommandArgs {
                        fork_entry_id: None,
                        provider,
                    },
                )
            }
            "auth.login-device" | "auth.cancel-login" => {
                let provider = parts.next().map(str::to_string);
                if provider.is_none() {
                    self.reject_slash(
                        submitted_text,
                        if id == "auth.login-device" {
                            "usage: /login-device <provider>"
                        } else {
                            "usage: /login-cancel <provider>"
                        },
                    )
                } else {
                    self.dispatch_host_command(
                        id,
                        HostCommandArgs {
                            fork_entry_id: None,
                            provider,
                        },
                    )
                }
            }
            "auth.logout" => {
                let provider = parts.next().map(str::to_string);
                if provider.is_none() && self.model.active_provider.is_none() {
                    self.reject_slash(submitted_text, "usage: /logout <provider>")
                } else {
                    self.dispatch_host_command(
                        id,
                        HostCommandArgs {
                            fork_entry_id: None,
                            provider,
                        },
                    )
                }
            }
            _ => self.dispatch_host_command(id, HostCommandArgs::default()),
        }
    }

    fn dispatch_host_command(&mut self, id: &str, args: HostCommandArgs) -> Vec<Effect> {
        match action_for_host_command(id, args) {
            Some(action) => self.dispatch(action),
            None => Vec::new(),
        }
    }

    fn reject_slash(&mut self, submitted_text: &str, usage: &str) -> Vec<Effect> {
        self.editor.restore_text(submitted_text);
        self.refresh_suggestions();
        self.status = usage.to_string();
        Vec::new()
    }
}
