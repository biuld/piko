//! Focus manager and input routing.
//!
//! Follows architecture.md Input Layer design:
//! - P1: Global Esc/Enter — always checked first, regardless of focus
//! - P2: Focus Owner — stack top handles keys; Capture blocks, Passive passes through
//! - P3: Editor — receives keys when no Capture panel is active

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use piko_protocol::ApprovalDecision;

use crate::{
    app::{
        AppMode, AppState,
        command::{
            Action, AgentPanelAction, AppAction, ApprovalAction, EditorAction, ModelAction,
            NotificationAction, SessionAction, SurfaceAction, TimelineAction,
            ToolInteractionAction, TreeAction,
        },
    },
    input::keymap::{KeyAction, Keymap},
};

// ── FocusManager ─────────────────────────────────────────────────────────────

/// LIFO stack of AppMode values. Stack bottom is always `Chat` (Editor).
/// Pushing opens a surface; popping closes it.
pub struct FocusManager {
    stack: Vec<AppMode>,
    pub last_esc_pressed: Option<Instant>,
}

impl FocusManager {
    pub fn new() -> Self {
        Self {
            stack: vec![AppMode::Chat],
            last_esc_pressed: None,
        }
    }

    pub fn active_mode(&self) -> AppMode {
        self.stack.last().copied().unwrap_or(AppMode::Chat)
    }

    pub fn push(&mut self, mode: AppMode) {
        if self.stack.last() != Some(&mode) {
            self.stack.push(mode);
        }
    }

    pub fn pop(&mut self) -> Option<AppMode> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    pub fn clear_to_chat(&mut self) {
        self.stack.truncate(1);
    }

    /// A Capture-style surface is active (not Chat).
    pub fn is_blocking_surface_active(&self) -> bool {
        self.active_mode() != AppMode::Chat
    }
}

// ── InputRouter ──────────────────────────────────────────────────────────────

pub struct InputRouter;

mod router;
#[cfg(test)]
mod tests;
