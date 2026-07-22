//! Directed island ↔ shell messages (not a broadcast bus).
//!
//! Constructed by Entity island views under `crate::features`; consumed by
//! `DesktopApp::dispatch_island_msg`. Payloads stay shell-facing primitives so
//! shell never depends on feature VM types.
//!
//! **Product surface** — lives in `piko-gui`, not `piko-chrome`.
//! Focus variants map to chrome [`FocusMsg`] via [`IslandMessage`].
#![allow(dead_code)]

use piko_chrome::{FocusMsg, IslandMessage};

use crate::shell::workbench::IslandId;

/// Messages emitted by islands or shell and routed by [`crate::app::desktop_app::DesktopApp`].
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
    RenameSession {
        session_id: String,
    },
    DeleteSession {
        session_id: String,
    },
    TogglePinSession {
        session_id: String,
    },
}

impl IslandMessage for IslandMsg {
    type Id = IslandId;

    fn as_focus_msg(&self) -> Option<FocusMsg<IslandId>> {
        match self {
            IslandMsg::ClaimFocus => Some(FocusMsg::ClaimFocus),
            IslandMsg::FocusIsland { id } => Some(FocusMsg::FocusIsland { id: *id }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_variants_map_to_chrome_focus_msg() {
        assert_eq!(
            IslandMsg::ClaimFocus.as_focus_msg(),
            Some(FocusMsg::ClaimFocus)
        );
        assert_eq!(
            IslandMsg::FocusIsland {
                id: IslandId::Composer
            }
            .as_focus_msg(),
            Some(FocusMsg::FocusIsland {
                id: IslandId::Composer
            })
        );
        assert!(IslandMsg::SubmitComposer.as_focus_msg().is_none());
        assert!(IslandMsg::OpenDirectory.as_focus_msg().is_none());
    }
}
