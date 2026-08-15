//! Focus manager re-export and input routing.
//!
//! Focus stack type is generic (`piko_tui_layout::FocusManager<T>`); product
//! instantiates it as `FocusManager<AppMode>`.
//!
//! Priority:
//! - P1: Global Esc/Enter
//! - P2: Focus owner
//! - P3: Editor

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use piko_protocol::ApprovalDecision;

use crate::{
    app::{
        AppMode, AppState, SurfaceId,
        command::{
            Action, AppAction, ApprovalAction, EditorAction, ModelAction, NotificationAction,
            SessionAction, SurfaceAction, TimelineAction, ToolInteractionAction, TreeAction,
        },
    },
    input::keymap::{KeyAction, Keymap},
    navigation::SurfaceInputProfile,
};

pub use crate::navigation::FocusManager;

pub struct InputRouter;

mod router;
#[cfg(test)]
mod tests;
