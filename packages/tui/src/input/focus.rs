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
    app::{AppMode, AppState, command::Action},
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

impl InputRouter {
    /// Route a key event through the 3-layer priority chain:
    ///
    /// ```
    /// P1: Global Esc/Enter → handle_global_key()
    ///   ├─ Esc: Approval decline, close surface, cancel suggestions, cancel turn, open tree
    ///   └─ Enter: Approval accept, confirm selection, accept suggestion, submit
    ///
    /// P2: Focus Owner → handle_focus_key()
    ///   ├─ If Capture: keys consumed by panel, nothing reaches Editor
    ///   └─ If Passive: unhandled keys pass through to Editor
    ///
    /// P3: Editor → handle_editor_key()
    ///   └─ Text input, cursor movement, history, timeline scroll, keyboard commands
    /// ```
    pub fn route_key(app: &mut AppState, keymap: &Keymap, key: KeyEvent) -> Option<Action> {
        let ka = keymap.action_for(key);

        // ═══ P1: Global Esc/Enter ═══
        if let Some(action) = Self::handle_global_key(app, ka, key) {
            return Some(action);
        }

        // ═══ P2: Focus Owner ═══
        let active = app.focus_manager.active_mode();
        if active != AppMode::Chat {
            if let Some(action) = Self::handle_focus_key(app, active, ka, key) {
                return Some(action);
            }
            // Capture panels: consume event, don't pass to Editor
            // (All non-Chat modes are Capture; Help/Status are theoretically
            // Passive per architecture but current code treats them as Capture too)
            return None;
        }

        // ═══ P3: Editor ═══
        Self::handle_editor_key(app, ka, key)
    }

    // ── P1: Global Esc/Enter ────────────────────────────────────────────────

    fn handle_global_key(
        app: &mut AppState,
        ka: Option<KeyAction>,
        key: KeyEvent,
    ) -> Option<Action> {
        // ── Esc (Cancel) ──
        if ka == Some(KeyAction::Cancel) || key.code == KeyCode::Esc {
            // 1. Blocking surface active → close it
            if app.focus_manager.is_blocking_surface_active() {
                return Some(Action::CloseSurface);
            }
            // 2. Suggestions visible → cancel them
            if app.has_suggestions() {
                return Some(Action::CancelSuggestions);
            }
            // 3. Active turn → cancel it
            if app.active_turn_id().is_some() {
                return Some(Action::Cancel);
            }
            // 4. Editor empty + double-Esc → open tree
            if app.editor.is_empty() {
                let now = Instant::now();
                let double_esc = app
                    .focus_manager
                    .last_esc_pressed
                    .map(|last| now.duration_since(last).as_millis() < 500)
                    .unwrap_or(false);
                if double_esc {
                    app.focus_manager.last_esc_pressed = None;
                    return Some(Action::OpenTree);
                }
                app.focus_manager.last_esc_pressed = Some(now);
            }
            return None;
        }

        // ── Enter ──
        if key.code == KeyCode::Enter || ka == Some(KeyAction::Submit) {
            // Let P2 handle Enter if a surface is active
            // (handled below in handle_focus_key)
        }

        None
    }

    // ── P2: Focus Owner ─────────────────────────────────────────────────────

    fn handle_focus_key(
        _app: &AppState,
        active: AppMode,
        ka: Option<KeyAction>,
        key: KeyEvent,
    ) -> Option<Action> {
        // Approval mode: special handling before generic surface routing
        if active == AppMode::Approval {
            return match key.code {
                KeyCode::Enter => Some(Action::ApprovalRespond(ApprovalDecision::Accept)),
                KeyCode::Esc => Some(Action::ApprovalRespond(ApprovalDecision::Decline)),
                KeyCode::Char('a' | 'A') => {
                    Some(Action::ApprovalRespond(ApprovalDecision::AcceptSession))
                }
                KeyCode::Char('w' | 'W') => {
                    Some(Action::ApprovalRespond(ApprovalDecision::AcceptWorkspace))
                }
                KeyCode::Char('p' | 'P') => {
                    Some(Action::ApprovalRespond(ApprovalDecision::AcceptPermanent))
                }
                _ => None,
            };
        }

        match active {
            // Filterable list surfaces: Commands, Tree, Sessions, Settings, Models
            AppMode::Commands
            | AppMode::Tree
            | AppMode::Sessions
            | AppMode::Settings
            | AppMode::Models => Self::handle_filterable_surface(key, ka),
            // Info panels: Status
            AppMode::Status => match ka {
                Some(KeyAction::SelectPrev) => Some(Action::SelectPrev),
                Some(KeyAction::SelectNext) => Some(Action::SelectNext),
                Some(KeyAction::Submit | KeyAction::Confirm) => Some(Action::ConfirmSelection),
                Some(KeyAction::Cancel) => Some(Action::CloseSurface),
                Some(KeyAction::Exit) => Some(Action::Quit),
                None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
                _ => None,
            },
            // Help: passive info panel
            AppMode::Help => match ka {
                Some(KeyAction::Cancel | KeyAction::Submit | KeyAction::Confirm) => {
                    Some(Action::CloseSurface)
                }
                Some(KeyAction::Exit) => Some(Action::Quit),
                None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
                _ => None,
            },
            AppMode::Chat | AppMode::Approval => None,
        }
    }

    /// Shared logic for all filterable list surfaces (Commands, Tree, Sessions, Settings, Models).
    fn handle_filterable_surface(key: KeyEvent, ka: Option<KeyAction>) -> Option<Action> {
        // Character input → filter append
        if let KeyCode::Char(ch) = key.code
            && !key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT)
        {
            return Some(Action::FilterAppend(ch));
        }
        // Backspace → filter backspace
        if key.code == KeyCode::Backspace {
            return Some(Action::FilterBackspace);
        }
        // Keymap-driven actions
        match ka {
            Some(KeyAction::SelectPrev) => Some(Action::SelectPrev),
            Some(KeyAction::SelectNext) => Some(Action::SelectNext),
            Some(KeyAction::Submit | KeyAction::Confirm) => Some(Action::ConfirmSelection),
            Some(KeyAction::Cancel) => Some(Action::CloseSurface),
            Some(KeyAction::Exit) => Some(Action::Quit),
            None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
            _ => None,
        }
    }

    // ── P3: Editor ──────────────────────────────────────────────────────────

    fn handle_editor_key(app: &AppState, ka: Option<KeyAction>, key: KeyEvent) -> Option<Action> {
        // Autocomplete intercepts Up/Down/Tab/Enter when suggestions are visible
        if app.has_suggestions() {
            match ka {
                Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                    return Some(Action::SuggestionSelectPrev);
                }
                Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                    return Some(Action::SuggestionSelectNext);
                }
                Some(KeyAction::Complete) => {
                    return Some(Action::AcceptSuggestion);
                }
                Some(KeyAction::Submit) => {
                    return Some(Action::AcceptAndSubmitSuggestion);
                }
                _ => {}
            }
        }

        // Standard editor inputs, timeline scroll, and keyboard commands
        match ka {
            Some(KeyAction::Exit) => Some(Action::Quit),
            Some(KeyAction::NewLine) => Some(Action::InsertNewline),
            Some(KeyAction::Sessions) => Some(Action::RequestSessions),
            Some(KeyAction::SessionTree) => Some(Action::OpenTree),
            Some(KeyAction::Commands) => Some(Action::OpenCommands),
            Some(KeyAction::Settings) => Some(Action::OpenSettings),
            Some(KeyAction::Status) => Some(Action::OpenStatus),
            Some(KeyAction::ApprovalAccept) => {
                Some(Action::ApprovalRespond(ApprovalDecision::Accept))
            }
            Some(KeyAction::ApprovalAcceptSession) => {
                Some(Action::ApprovalRespond(ApprovalDecision::AcceptSession))
            }
            Some(KeyAction::ApprovalAcceptWorkspace) => {
                Some(Action::ApprovalRespond(ApprovalDecision::AcceptWorkspace))
            }
            Some(KeyAction::ApprovalDecline) => {
                Some(Action::ApprovalRespond(ApprovalDecision::Decline))
            }
            Some(KeyAction::ClearNotifications) => Some(Action::ClearNotifications),
            Some(KeyAction::HistoryPrev) => Some(Action::HistoryPrev),
            Some(KeyAction::HistoryNext) => Some(Action::HistoryNext),
            Some(KeyAction::DeleteBackward) => Some(Action::DeleteBackward),
            Some(KeyAction::DeleteForward) => Some(Action::DeleteForward),
            Some(KeyAction::Submit) => Some(Action::Submit),
            Some(KeyAction::Complete) => Some(Action::AcceptSuggestion),
            Some(KeyAction::CursorLeft) => Some(Action::CursorLeft),
            Some(KeyAction::CursorRight) => Some(Action::CursorRight),
            Some(KeyAction::CursorLineStart) => Some(Action::CursorLineStart),
            Some(KeyAction::CursorLineEnd) => Some(Action::CursorLineEnd),
            Some(KeyAction::Cancel) => Some(Action::Cancel),
            Some(KeyAction::TimelinePageUp) => Some(Action::TimelineScrollUp(8)),
            Some(KeyAction::TimelinePageDown) => Some(Action::TimelineScrollDown(8)),
            Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                Some(Action::TimelineScrollUp(1))
            }
            Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                Some(Action::TimelineScrollDown(1))
            }
            Some(KeyAction::TimelineLatest) => Some(Action::TimelineJumpLatest),
            Some(KeyAction::Help) => Some(Action::OpenHelp),
            Some(KeyAction::Models) => Some(Action::RequestModels),
            None => {
                if let KeyCode::Char(ch) = key.code {
                    Some(Action::InsertChar(ch))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
