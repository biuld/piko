//! Neutral host command catalog (see `docs/host-command-catalog-design.md`).
//!
//! This module emits only frontend-neutral product commands: session
//! lifecycle, auth, runtime/agent, and model/thinking "set" intents. It must
//! never mention TUI, GUI, ratatui, or GPUI, and must never carry UI-opener
//! semantics (Settings/Quit/Help/Tree/Models/Thinking openers, slash names,
//! or palette visibility flags) — those are frontend-owned presentation
//! commands layered on top by each client.
//!
//! Ids are stable and dotted (`session.new`, `auth.login`, ...). Frontends
//! own the `id -> Command` / `id -> ClientIntent` mapping table used to
//! actually execute a row.

use crate::api::HostCommandInvoke::{self, Args, Confirm, Immediate};
use crate::api::{HostCommandArg, HostCommandArgKind, HostCommandDescriptor, HostCommandGroup};

pub fn command_catalog() -> Vec<HostCommandDescriptor> {
    vec![
        // ── Session ──────────────────────────────────────────────────────
        item(
            "session.new",
            "New session",
            "Create a new session in the current working directory",
            Immediate,
            HostCommandGroup::Session,
        ),
        item(
            "session.fork",
            "Fork session",
            "Create a new branch from a session tree entry",
            Immediate,
            HostCommandGroup::Session,
        ),
        item(
            "session.clone",
            "Clone session",
            "Clone the current session at its current leaf",
            Immediate,
            HostCommandGroup::Session,
        ),
        item(
            "session.rename",
            "Rename session",
            "Set a new name for the current session",
            Args {
                schema: vec![arg("name", HostCommandArgKind::String, true)],
            },
            HostCommandGroup::Session,
        ),
        item(
            "session.delete",
            "Delete session",
            "Permanently delete the current session",
            Confirm,
            HostCommandGroup::Session,
        ),
        item(
            "session.import",
            "Import session",
            "Import a session from a JSONL file",
            Args {
                schema: vec![arg("path", HostCommandArgKind::String, true)],
            },
            HostCommandGroup::Session,
        ),
        item(
            "session.export",
            "Export session",
            "Get the file path of the current session",
            Immediate,
            HostCommandGroup::Session,
        ),
        // ── Auth ─────────────────────────────────────────────────────────
        item(
            "auth.login",
            "Sign in",
            "Start OAuth login for a model provider",
            Args {
                schema: vec![arg("provider", HostCommandArgKind::Provider, false)],
            },
            HostCommandGroup::Auth,
        ),
        item(
            "auth.logout",
            "Sign out",
            "Remove stored credentials for a model provider",
            Args {
                schema: vec![arg("provider", HostCommandArgKind::Provider, false)],
            },
            HostCommandGroup::Auth,
        ),
        // ── Agent / runtime ──────────────────────────────────────────────
        item(
            "session.compact",
            "Compact session",
            "Reduce transcript size while preserving context",
            Immediate,
            HostCommandGroup::Runtime,
        ),
        // ── Model / thinking (set, not browse UI) ───────────────────────
        item(
            "model.set",
            "Set model",
            "Set the default provider and model",
            Args {
                schema: vec![
                    arg("provider", HostCommandArgKind::String, true),
                    arg("model", HostCommandArgKind::String, true),
                ],
            },
            HostCommandGroup::Model,
        ),
        item(
            "thinking.set",
            "Set thinking level",
            "Set the default reasoning/thinking level",
            Args {
                schema: vec![arg("level", HostCommandArgKind::String, true)],
            },
            HostCommandGroup::Model,
        ),
    ]
}

fn arg(name: &str, kind: HostCommandArgKind, required: bool) -> HostCommandArg {
    HostCommandArg {
        name: name.to_string(),
        kind,
        required,
    }
}

fn item(
    id: &str,
    title: &str,
    detail: &str,
    invoke: HostCommandInvoke,
    group: HostCommandGroup,
) -> HostCommandDescriptor {
    HostCommandDescriptor {
        id: id.to_string(),
        title: title.to_string(),
        detail: detail.to_string(),
        invoke,
        group: Some(group),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Presentation-only ids must never leak into the neutral host catalog.
    const FORBIDDEN_IDS: &[&str] = &[
        "quit",
        "help",
        "settings",
        "tree",
        "tools.toggle",
        "notifications.clear",
        "sessions",
        "models",
        "thinking",
        "status",
        "agents",
        "commands",
    ];

    #[test]
    fn catalog_excludes_presentation_ids() {
        let catalog = command_catalog();
        for forbidden in FORBIDDEN_IDS {
            assert!(
                !catalog.iter().any(|c| c.id == *forbidden),
                "presentation-only id {forbidden:?} leaked into host catalog"
            );
        }
    }

    #[test]
    fn catalog_titles_and_details_are_frontend_neutral() {
        let banned = ["tui", "gui", "ratatui", "gpui", "palette", "slash"];
        for command in command_catalog() {
            let haystack = format!("{} {}", command.title, command.detail).to_lowercase();
            for word in banned {
                assert!(
                    !haystack.contains(word),
                    "command {:?} mentions frontend-coupled word {word:?}",
                    command.id
                );
            }
        }
    }

    #[test]
    fn catalog_ids_are_stable_and_unique() {
        let catalog = command_catalog();
        let mut ids: Vec<&str> = catalog.iter().map(|c| c.id.as_str()).collect();
        let unique_count = {
            ids.sort_unstable();
            ids.dedup();
            ids.len()
        };
        assert_eq!(unique_count, catalog.len());
        assert!(catalog.iter().any(|c| c.id == "session.new"));
        assert!(catalog.iter().any(|c| c.id == "model.set"));
        assert!(catalog.iter().any(|c| c.id == "thinking.set"));
    }
}
