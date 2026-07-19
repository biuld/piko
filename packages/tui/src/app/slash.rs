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
        let target = self
            .command_catalog
            .iter()
            .find(|entry| entry.slash == command)
            .map(|entry| entry.target.clone())?;

        let effects = match target {
            CommandTarget::Local(id) => self.dispatch(action_for_local_command(id)),
            CommandTarget::Host(id) => self.run_host_slash(&id, parts),
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
    ) -> Vec<Effect> {
        match id {
            "session.fork" => {
                let entry_id = parts
                    .next()
                    .map(ToString::to_string)
                    .or_else(|| self.tree.selected_filtered_entry_id());
                self.dispatch_host_command(
                    id,
                    HostCommandArgs {
                        fork_entry_id: entry_id,
                        provider: None,
                    },
                )
            }
            "session.rename" => {
                let name = parts.collect::<Vec<_>>().join(" ");
                if name.is_empty() {
                    self.status = "usage: /rename <session name>".to_string();
                    Vec::new()
                } else {
                    self.dispatch(SlashAction::Rename(name).into())
                }
            }
            "session.import" => {
                let path = parts.collect::<Vec<_>>().join(" ");
                if path.is_empty() {
                    self.status = "usage: /import <jsonl path>".to_string();
                    Vec::new()
                } else {
                    self.dispatch(SlashAction::Import(path).into())
                }
            }
            "session.export" => {
                if let Some(ref session_id) = self.session.id {
                    self.status = format!(
                        "Session saved in ~/.piko/agent/sessions/ under ID: {}",
                        session_id
                    );
                } else {
                    self.status = "No active session".to_string();
                }
                Vec::new()
            }
            "session.delete" => {
                if parts.next() == Some("confirm") {
                    self.dispatch(SlashAction::Delete.into())
                } else {
                    self.status = "usage: /delete confirm".to_string();
                    Vec::new()
                }
            }
            "auth.login" | "auth.logout" => {
                let provider = parts.next().map(|s| s.to_string());
                self.dispatch_host_command(
                    id,
                    HostCommandArgs {
                        fork_entry_id: None,
                        provider,
                    },
                )
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
}
