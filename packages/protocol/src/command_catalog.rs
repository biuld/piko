//! Frontend-neutral host command catalog DTOs.
//!
//! The host catalog advertises *what* neutral product commands exist. It
//! carries no frontend wording, no slash aliases, and no UI-opener actions
//! (Settings/Quit/Help/Tree/Models/Thinking openers, etc.) — those are
//! frontend-owned presentation concerns. See
//! `docs/host-command-catalog-design.md` for the full rationale.
//!
//! Each descriptor exposes a stable dotted `id` only (decision: style A,
//! id-only). Clients maintain their own `id -> Command` / `id -> ClientIntent`
//! table to actually execute a row; the catalog is a discovery/documentation
//! list, not an effect enum.

use serde::{Deserialize, Serialize};

/// A single neutral, runnable host command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostCommandDescriptor {
    /// Stable dotted id, e.g. `"session.new"`. Frontends map this to a
    /// concrete `Command` / `ClientIntent`; it is never rendered directly.
    pub id: String,
    /// Neutral English product title (no TUI/GUI/ratatui/GPUI wording).
    pub title: String,
    /// Neutral English description of what the command does.
    pub detail: String,
    /// How the frontend must invoke this command.
    pub invoke: HostCommandInvoke,
    /// Optional host-suggested grouping hint; frontends may ignore it and
    /// group locally instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<HostCommandGroup>,
}

/// How a frontend must invoke a catalog entry. This describes invocation
/// shape only — never a widget/menu to open (nested pickers such as model or
/// thinking-level lists are frontend UX, not an invoke kind).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostCommandInvoke {
    /// No arguments; the frontend fires it directly.
    Immediate,
    /// Requires arguments described by an explicit, fixed arg list.
    ///
    /// v1 intentionally uses a small closed set of arg kinds instead of full
    /// JSON Schema (see design §9 non-goals).
    Args { schema: Vec<HostCommandArg> },
    /// The frontend must obtain user confirmation before firing; no
    /// additional input is collected beyond the confirmation itself.
    Confirm,
}

/// One explicit argument slot for an `Args` invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostCommandArg {
    pub name: String,
    pub kind: HostCommandArgKind,
    #[serde(default)]
    pub required: bool,
}

/// Known argument value shapes (v1: explicit kinds, not JSON Schema).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostCommandArgKind {
    String,
    Provider,
    SessionEntryId,
}

/// Host-suggested grouping. Frontends may use it as-is or group locally.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostCommandGroup {
    Session,
    Auth,
    Runtime,
    Model,
}
