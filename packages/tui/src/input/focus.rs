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
            Action, AppAction, ApprovalAction, EditorAction, ModelAction, NotificationAction,
            SessionAction, SurfaceAction, TimelineAction, ToolInteractionAction, TreeAction,
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
                            return Some(SurfaceAction::Confirm.into());
                        }
                    }
                    KeyCode::Up | KeyCode::Left | KeyCode::BackTab => {
                        return Some(SurfaceAction::SelectPrev.into());
                    }
                    KeyCode::Down | KeyCode::Right | KeyCode::Tab => {
                        return Some(SurfaceAction::SelectNext.into());
                    }
                    KeyCode::Backspace => return Some(SurfaceAction::FilterBackspace.into()),
                    KeyCode::Char(ch) => {
                        if ch == 'C' && key.modifiers.contains(KeyModifiers::CONTROL) {
                            app.summary_prompt = None;
                            app.pop_focus();
                            return None;
                        }
                        if let Some(state) = &mut app.summary_prompt
                            && state.input_active()
                        {
                            return Some(SurfaceAction::FilterAppend(ch).into());
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
                return Some(ApprovalAction::Respond(ApprovalDecision::Decline).into());
            }
            if app.focus_manager.active_mode() == AppMode::ToolInteraction {
                return Some(ToolInteractionAction::Cancel.into());
            }
            // 1. Blocking surface active → close it
            if app.focus_manager.is_blocking_surface_active() {
                return Some(SurfaceAction::Close.into());
            }
            // 2. Suggestions visible → cancel them
            if app.has_suggestions() {
                return Some(EditorAction::CancelSuggestions.into());
            }
            // 3. Active turn → cancel it
            if app.active_turn_id().is_some() {
                return Some(EditorAction::Cancel.into());
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
                    return Some(SurfaceAction::OpenTree.into());
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
                    Some(ApprovalAction::Respond(ApprovalDecision::Accept).into())
                }
                Some(KeyAction::ApprovalAcceptSession) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptSession).into())
                }
                Some(KeyAction::ApprovalAcceptWorkspace) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptWorkspace).into())
                }
                Some(KeyAction::ApprovalDecline) => {
                    Some(ApprovalAction::Respond(ApprovalDecision::Decline).into())
                }
                _ => None,
            } {
                return Some(action);
            }
            return match key.code {
                KeyCode::Enter => Some(ApprovalAction::Respond(ApprovalDecision::Accept).into()),
                KeyCode::Esc => Some(ApprovalAction::Respond(ApprovalDecision::Decline).into()),
                KeyCode::Char('a' | 'A') => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptSession).into())
                }
                KeyCode::Char('w' | 'W') => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptWorkspace).into())
                }
                KeyCode::Char('p' | 'P') => {
                    Some(ApprovalAction::Respond(ApprovalDecision::AcceptPermanent).into())
                }
                _ => None,
            };
        }

        if active == AppMode::ToolInteraction {
            return match key.code {
                KeyCode::Enter => Some(ToolInteractionAction::Submit.into()),
                KeyCode::Esc => Some(ToolInteractionAction::Cancel.into()),
                KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
                    Some(ToolInteractionAction::NextStep.into())
                }
                KeyCode::BackTab | KeyCode::Up | KeyCode::Left => {
                    Some(ToolInteractionAction::PrevStep.into())
                }
                KeyCode::Backspace => Some(SurfaceAction::FilterBackspace.into()),
                KeyCode::Char(ch)
                    if ch.is_ascii_digit()
                        && !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    ch.to_digit(10)
                        .and_then(|digit| digit.checked_sub(1))
                        .map(|idx| ToolInteractionAction::Choice(idx as usize).into())
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    Some(SurfaceAction::FilterAppend(ch).into())
                }
                _ => None,
            };
        }

        match active {
            // Filterable list surfaces: Tree, Sessions, Settings, Models, AuthSelector
            AppMode::Tree
            | AppMode::Sessions
            | AppMode::Settings
            | AppMode::Models
            | AppMode::AuthSelector => {
                if active == AppMode::Tree {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(if key.modifiers.contains(KeyModifiers::SHIFT) {
                            TreeAction::FilterCycleBackward.into()
                        } else {
                            TreeAction::FilterCycleForward.into()
                        });
                    }
                    match ka {
                        Some(KeyAction::TreeFoldOrUp) => return Some(TreeAction::FoldOrUp.into()),
                        Some(KeyAction::TreeUnfoldOrDown) => {
                            return Some(TreeAction::UnfoldOrDown.into());
                        }
                        Some(KeyAction::TreeEditLabel) => {
                            return Some(TreeAction::EditLabel.into());
                        }
                        Some(KeyAction::TreeToggleLabelTimestamp) => {
                            return Some(TreeAction::ToggleLabelTimestamp.into());
                        }
                        Some(KeyAction::TreeFilterCycleForward) => {
                            return Some(TreeAction::FilterCycleForward.into());
                        }
                        Some(KeyAction::TreeFilterCycleBackward) => {
                            return Some(TreeAction::FilterCycleBackward.into());
                        }
                        _ => {
                            if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Left
                            {
                                return Some(TreeAction::FoldOrUp.into());
                            } else if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && key.code == KeyCode::Right
                            {
                                return Some(TreeAction::UnfoldOrDown.into());
                            } else if key.code == KeyCode::Char('L')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(TreeAction::EditLabel.into());
                            } else if key.code == KeyCode::Char('T')
                                && key.modifiers.contains(KeyModifiers::SHIFT)
                            {
                                return Some(TreeAction::ToggleLabelTimestamp.into());
                            } else if key.code == KeyCode::Char('o')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                if key.modifiers.contains(KeyModifiers::SHIFT) {
                                    return Some(TreeAction::FilterCycleBackward.into());
                                } else {
                                    return Some(TreeAction::FilterCycleForward.into());
                                }
                            }
                        }
                    }
                }

                if active == AppMode::Sessions {
                    if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
                        return Some(SessionAction::ToggleScope.into());
                    }
                    if let Some(action) = ka {
                        match action {
                            KeyAction::SessionToggleNamedFilter => {
                                return Some(SessionAction::ToggleNamed.into());
                            }
                            KeyAction::SessionTogglePath => {
                                return Some(SessionAction::TogglePath.into());
                            }
                            _ => {}
                        }
                    }
                }
                Self::handle_filterable_surface(key, ka)
            }
            // Info panels: Status
            AppMode::Status => match ka {
                Some(KeyAction::SelectPrev) => Some(SurfaceAction::SelectPrev.into()),
                Some(KeyAction::SelectNext) => Some(SurfaceAction::SelectNext.into()),
                Some(KeyAction::Submit | KeyAction::Confirm) => Some(SurfaceAction::Confirm.into()),
                Some(KeyAction::Cancel) => Some(SurfaceAction::Close.into()),
                Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
                None if matches!(key.code, KeyCode::Char('q')) => Some(SurfaceAction::Close.into()),
                _ => None,
            },
            // Help: passive info panel
            AppMode::Help => match ka {
                Some(KeyAction::Cancel | KeyAction::Submit | KeyAction::Confirm) => {
                    Some(SurfaceAction::Close.into())
                }
                Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
                None if matches!(key.code, KeyCode::Char('q')) => Some(SurfaceAction::Close.into()),
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
            return Some(SurfaceAction::FilterAppend(ch).into());
        }
        // Backspace → filter backspace
        if key.code == KeyCode::Backspace {
            return Some(SurfaceAction::FilterBackspace.into());
        }
        // Keymap-driven actions
        match ka {
            Some(KeyAction::SelectPrev) => Some(SurfaceAction::SelectPrev.into()),
            Some(KeyAction::SelectNext) => Some(SurfaceAction::SelectNext.into()),
            Some(KeyAction::Submit | KeyAction::Confirm) => Some(SurfaceAction::Confirm.into()),
            Some(KeyAction::Cancel) => Some(SurfaceAction::Close.into()),
            Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
            None if matches!(key.code, KeyCode::Char('q')) => Some(SurfaceAction::Close.into()),
            _ => None,
        }
    }

    // ── P3: Editor ──────────────────────────────────────────────────────────

    fn handle_editor_key(app: &AppState, ka: Option<KeyAction>, key: KeyEvent) -> Option<Action> {
        // Autocomplete intercepts Up/Down/Tab/Enter when suggestions are visible
        if app.has_suggestions() {
            match ka {
                Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                    return Some(EditorAction::SuggestionSelectPrev.into());
                }
                Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                    return Some(EditorAction::SuggestionSelectNext.into());
                }
                Some(KeyAction::Complete) => {
                    return Some(EditorAction::SuggestionSelectNext.into());
                }
                Some(KeyAction::ThinkingCycle) => {
                    return Some(EditorAction::SuggestionSelectPrev.into());
                }
                Some(KeyAction::Submit) => {
                    return Some(EditorAction::AcceptAndSubmitSuggestion.into());
                }
                _ => {}
            }
        }

        // Standard editor inputs, timeline scroll, and keyboard commands
        match ka {
            Some(KeyAction::Exit) => Some(AppAction::Quit.into()),
            Some(KeyAction::NewLine) => Some(EditorAction::InsertNewline.into()),
            Some(KeyAction::Sessions) => Some(SessionAction::RequestList.into()),
            Some(KeyAction::SessionTree) => Some(SurfaceAction::OpenTree.into()),
            Some(KeyAction::Commands) => Some(EditorAction::OpenCommands.into()),
            Some(KeyAction::Settings) => Some(SurfaceAction::OpenSettings.into()),
            Some(KeyAction::Status) => Some(SurfaceAction::OpenStatus.into()),
            Some(KeyAction::ClearNotifications) => Some(NotificationAction::Clear.into()),
            Some(KeyAction::HistoryPrev) => Some(EditorAction::HistoryPrev.into()),
            Some(KeyAction::HistoryNext) => Some(EditorAction::HistoryNext.into()),
            Some(KeyAction::DeleteBackward) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteForward) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) && app.editor.is_empty() {
                    Some(AppAction::Quit.into())
                } else {
                    Some(EditorAction::DeleteForward.into())
                }
            }
            Some(KeyAction::DeleteWordBackward) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteWordForward) => Some(EditorAction::DeleteForward.into()),
            Some(KeyAction::DeleteToLineStart) => Some(EditorAction::DeleteBackward.into()),
            Some(KeyAction::DeleteToLineEnd) => Some(EditorAction::DeleteForward.into()),
            Some(KeyAction::Submit) => Some(EditorAction::Submit.into()),
            Some(KeyAction::Complete) => Some(EditorAction::AcceptSuggestion.into()),
            Some(KeyAction::CursorLeft | KeyAction::CursorWordLeft) => {
                Some(EditorAction::CursorLeft.into())
            }
            Some(KeyAction::CursorRight | KeyAction::CursorWordRight) => {
                Some(EditorAction::CursorRight.into())
            }
            Some(KeyAction::CursorLineStart) => Some(EditorAction::CursorLineStart.into()),
            Some(KeyAction::CursorLineEnd) => Some(EditorAction::CursorLineEnd.into()),
            Some(KeyAction::Cancel | KeyAction::Clear | KeyAction::Interrupt) => {
                Some(EditorAction::Cancel.into())
            }
            Some(KeyAction::TimelinePageUp) => Some(TimelineAction::ScrollUp(8).into()),
            Some(KeyAction::TimelinePageDown) => Some(TimelineAction::ScrollDown(8).into()),
            Some(KeyAction::SelectPrev | KeyAction::TimelineUp) => {
                Some(TimelineAction::ScrollUp(1).into())
            }
            Some(KeyAction::SelectNext | KeyAction::TimelineDown) => {
                Some(TimelineAction::ScrollDown(1).into())
            }
            Some(KeyAction::TimelineLatest) => Some(TimelineAction::JumpLatest.into()),
            Some(KeyAction::Help) => Some(SurfaceAction::OpenHelp.into()),
            Some(KeyAction::Models) => Some(ModelAction::RequestList.into()),
            None => {
                if let KeyCode::Char(ch) = key.code {
                    Some(EditorAction::InsertChar(ch).into())
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

        assert!(matches!(
            action,
            Some(Action::Editor(EditorAction::InsertChar('j')))
        ));
    }
}
