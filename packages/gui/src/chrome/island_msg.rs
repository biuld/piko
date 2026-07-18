//! Directed island ↔ chrome messages (not a broadcast bus).
//!
//! Constructed by the Entity island views under `crate::islands`; consumed
//! by `DesktopApp::dispatch_island_msg`. No island is wired into the docked
//! Workbench render tree yet, so variants may be temporarily unconstructed.
#![allow(dead_code)]

use crate::chrome::IslandId;
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
    NewSession,
    SelectAgent {
        agent_instance_id: String,
    },
    TreeActivate {
        entry_id: String,
    },
    TreeToggleExpand {
        entry_id: String,
    },
    SwitchBranch,
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
    CycleModel,
    CycleThinking,
}
