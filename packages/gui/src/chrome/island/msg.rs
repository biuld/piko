//! Directed island ↔ chrome messages (not a broadcast bus).
//!
//! Constructed by the Entity island views under `crate::islands`; consumed
//! by `DesktopApp::dispatch_island_msg`. No island is wired into the docked
//! Workbench render tree yet, so variants may be temporarily unconstructed.
#![allow(dead_code)]

use crate::chrome::workbench::IslandId;
use crate::islands::ActivityItem;

/// Messages emitted by islands or chrome and routed by [`DesktopApp`].
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
    ActivityActivate {
        item: ActivityItem,
    },
    SubmitComposer,
    CancelTurn,
}
