//! Directed island ↔ shell messages (not a broadcast bus).
//!
//! Constructed by Entity island views under `crate::features`; consumed by
//! `DesktopApp::dispatch_island_msg`. Payloads stay shell-owned primitives so
//! shell never depends on feature VM types.
#![allow(dead_code)]

use crate::shell::workbench::IslandId;

/// Messages emitted by islands or shell and routed by [`DesktopApp`].
#[derive(Debug, Clone)]
pub enum IslandMsg {
    /// Claim keyboard focus for this island (pointer down / activate).
    ClaimFocus,
    FocusIsland {
        id: IslandId,
    },
    OpenSession {
        session_id: String,
    },
    /// Create a Session in the given working directory.
    NewSession {
        cwd: String,
    },
    /// Pick a directory; create a Session there unless the cwd is already listed.
    OpenDirectory,
    SelectAgent {
        agent_instance_id: String,
    },
    TreeActivate {
        entry_id: String,
    },
    TreeToggleExpand {
        entry_id: String,
    },
    JumpToLatest,
    ToggleToolDetail {
        row_id: String,
    },
    ToggleActivity,
    /// Activate an Activity Center row (feature projects VM → these fields).
    ActivityActivate {
        agent_instance_id: Option<String>,
        /// When set, host should sync HostPrompt presentation for this id.
        prompt_id: Option<String>,
    },
    SubmitComposer,
    CancelTurn,
}
