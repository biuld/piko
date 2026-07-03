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

        // ── P2: Focus Owner ═══
        let active = app.focus_manager.active_mode();
        if active != AppMode::Chat {
            // Check if SummaryPrompt overrides
            if active == AppMode::SummaryPrompt {
                match key.code {
                    KeyCode::Esc => {
                        if let Some(state) = &mut app.summary_prompt
                            && state.input_active()
                        {
                            state.set_input_active(false);
                            return None;
                        }
                        app.summary_prompt = None;
                        app.pop_focus();
                        return None;
                    }
                    KeyCode::Enter => {
                        if app.summary_prompt.is_some() {
                            return Some(Action::ConfirmSelection);
                        }
                    }
                    KeyCode::Up | KeyCode::Left | KeyCode::BackTab => {
                        return Some(Action::SelectPrev);
                    }
                    KeyCode::Down | KeyCode::Right | KeyCode::Tab => {
                        return Some(Action::SelectNext);
                    }
                    KeyCode::Backspace => return Some(Action::FilterBackspace),
                    KeyCode::Char(ch) => {
                        if ch == 'C' && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.summary_prompt = None;
                            app.pop_focus();
                            return None;
                        }
                        if let Some(state) = &mut app.summary_prompt
                            && state.input_active()
                        {
                            return Some(Action::FilterAppend(ch));
                        }
                    }
                    _ => {}
                }
                // Don't pass through if active
                return None;
            }

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
            if app.focus_manager.active_mode() == AppMode::Approval {
                return Some(Action::ApprovalRespond(ApprovalDecision::Decline));
            }
            if app.focus_manager.active_mode() == AppMode::ToolInteraction {
                return Some(Action::ToolInteractionCancel);
            }
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
            if let Some(action) = match ka {
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
                _ => None,
            } {
                return Some(action);
            }
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

        if active == AppMode::ToolInteraction {
            return match key.code {
                KeyCode::Enter => Some(Action::ToolInteractionSubmit),
                KeyCode::Esc => Some(Action::ToolInteractionCancel),
                KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                    Some(Action::ToolInteractionNextStep)
                }
                KeyCode::BackTab | KeyCode::Up | KeyCode::Left => {
                    Some(Action::ToolInteractionPrevStep)
                }
                KeyCode::Backspace => Some(Action::FilterBackspace),
                KeyCode::Char(ch)
                    if ch.is_ascii_digit()
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    ch.to_digit(10)
                        .and_then(|digit| digit.checked_sub(1))
                        .map(|idx| Action::ToolInteractionChoice(idx as usize))
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(Action::FilterAppend(ch))
                }
                _ => None,
            };
        }

        match active {
            // Filterable list surfaces: Tree, Sessions, Settings, Models
            AppMode::Tree | AppMode::Sessions | AppMode::Settings | AppMode::Models => {
                if active == AppMode::Tree {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(if key.modifiers.contains(KeyModifiers::SHIFT) {
                            Action::TreeFilterCycleBackward
                        } else {
                            Action::TreeFilterCycleForward
                        });
                    }
                    match ka {
                        Some(KeyAction::TreeFoldOrUp) => return Some(Action::TreeFoldOrUp),
                        Some(KeyAction::TreeUnfoldOrDown) => return Some(Action::TreeUnfoldOrDown),
                        Some(KeyAction::TreeEditLabel) => return Some(Action::TreeEditLabel),
                        Some(KeyAction::TreeToggleLabelTimestamp) => {
                            return Some(Action::TreeToggleLabelTimestamp);
                        }
                        Some(KeyAction::TreeFilterCycleForward) => {
                            return Some(Action::TreeFilterCycleForward);
                        }
                        Some(KeyAction::TreeFilterCycleBackward) => {
                            return Some(Action::TreeFilterCycleBackward);
                        }
                        _ => {
                            if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Left
                            {
                                return Some(Action::TreeFoldOrUp);
                            } else if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Right
                            {
                                return Some(Action::TreeUnfoldOrDown);
                            } else if key.code == KeyCode::Char('L')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(Action::TreeEditLabel);
                            } else if key.code == KeyCode::Char('T')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(Action::TreeToggleLabelTimestamp);
                            } else if key.code == KeyCode::Char('o')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                if key.modifiers.contains(KeyModifiers::SHIFT) {
                                    return Some(Action::TreeFilterCycleBackward);
                                } else {
                                    return Some(Action::TreeFilterCycleForward);
                                }
                            }
                        }
                    }
                }

                if active == AppMode::Sessions {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(Action::SessionToggleScope);
                    }
                    if let Some(action) = ka {
                        match action {
                            KeyAction::SessionToggleNamedFilter => {
                                return Some(Action::SessionToggleNamed);
                            }
                            KeyAction::SessionTogglePath => return Some(Action::SessionTogglePath),
                            _ => {}
                        }
                    }
                }
                Self::handle_filterable_surface(key, ka)
            }
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
            AppMode::Chat
            | AppMode::Approval
            | AppMode::ToolInteraction
            | AppMode::SummaryPrompt => None,
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
                    return Some(Action::SuggestionSelectNext);
                }
                Some(KeyAction::ThinkingCycle) => {
                    return Some(Action::SuggestionSelectPrev);
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
            Some(KeyAction::ClearNotifications) => Some(Action::ClearNotifications),
            Some(KeyAction::HistoryPrev) => Some(Action::HistoryPrev),
            Some(KeyAction::HistoryNext) => Some(Action::HistoryNext),
            Some(KeyAction::DeleteBackward) => Some(Action::DeleteBackward),
            Some(KeyAction::DeleteForward) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && app.editor.is_empty() {
                    Some(Action::Quit)
                } else {
                    Some(Action::DeleteForward)
                }
            }
            Some(KeyAction::DeleteWordBackward) => Some(Action::DeleteBackward),
            Some(KeyAction::DeleteWordForward) => Some(Action::DeleteForward),
            Some(KeyAction::DeleteToLineStart) => Some(Action::DeleteBackward),
            Some(KeyAction::DeleteToLineEnd) => Some(Action::DeleteForward),
            Some(KeyAction::Submit) => Some(Action::Submit),
            Some(KeyAction::Complete) => Some(Action::AcceptSuggestion),
            Some(KeyAction::CursorLeft | KeyAction::CursorWordLeft) => Some(Action::CursorLeft),
            Some(KeyAction::CursorRight | KeyAction::CursorWordRight) => Some(Action::CursorRight),
            Some(KeyAction::CursorLineStart) => Some(Action::CursorLineStart),
            Some(KeyAction::CursorLineEnd) => Some(Action::CursorLineEnd),
            Some(KeyAction::Cancel | KeyAction::Clear | KeyAction::Interrupt) => {
                Some(Action::Cancel)
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppState, InitialOptions};
    use crossterm::event::{KeyEventKind, KeyEventState};
    use std::path::PathBuf;

    fn app() -> AppState {
        AppState::new(
            PathBuf::from("/tmp/piko-test"),
            None,
            false,
            InitialOptions::default(),
        )
    }

    #[test]
    fn plain_j_reaches_editor_as_text() {
        let mut app = app();
        let keymap = Keymap::default();
        let action = InputRouter::route_key(
            &mut app,
            &keymap,
            KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
        );

        assert!(matches!(action, Some(Action::InsertChar('j'))));
    }
}
