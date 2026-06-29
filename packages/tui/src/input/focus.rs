use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use piko_protocol::ApprovalDecision;

use crate::{
    action::Action,
    app::{AppMode, AppState},
    input::keymap::{KeyAction, Keymap},
};

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

    #[allow(dead_code)]
    pub fn pop_to(&mut self, mode: AppMode) {
        if let Some(pos) = self.stack.iter().position(|&m| m == mode) {
            self.stack.truncate(pos + 1);
        }
    }

    pub fn clear_to_chat(&mut self) {
        self.stack.truncate(1);
    }

    pub fn is_blocking_surface_active(&self) -> bool {
        let active = self.active_mode();
        active != AppMode::Chat
    }
}

pub struct InputRouter;

impl InputRouter {
    pub fn route_key(app: &mut AppState, keymap: &Keymap, key: KeyEvent) -> Option<Action> {
        let ka = keymap.action_for(key);

        // 1. Global / Escape Handling
        if ka == Some(KeyAction::Cancel) {
            if app.focus_manager.is_blocking_surface_active() {
                return Some(Action::CloseSurface);
            }
            if app.has_suggestions() {
                return Some(Action::CancelSuggestions);
            }
            if app.active_turn_id().is_some() {
                return Some(Action::Cancel);
            }
            if app.editor.is_empty() {
                let now = Instant::now();
                let double_esc = if let Some(last) = app.focus_manager.last_esc_pressed {
                    now.duration_since(last).as_millis() < 500
                } else {
                    false
                };
                if double_esc {
                    app.focus_manager.last_esc_pressed = None;
                    return Some(Action::OpenTree);
                } else {
                    app.focus_manager.last_esc_pressed = Some(now);
                    return None;
                }
            }
        }

        // 2. Active Mode (Surface) Routing
        let active_mode = app.focus_manager.active_mode();
        if active_mode == AppMode::Approval {
            if ka == Some(KeyAction::Cancel) || key.code == KeyCode::Esc {
                return Some(Action::ApprovalRespond(ApprovalDecision::Decline));
            }
            return match key.code {
                KeyCode::Enter => Some(Action::ApprovalRespond(ApprovalDecision::Accept)),
                KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::ApprovalRespond(ApprovalDecision::AcceptSession)),
                KeyCode::Char('w') | KeyCode::Char('W') => Some(Action::ApprovalRespond(ApprovalDecision::AcceptWorkspace)),
                KeyCode::Char('p') | KeyCode::Char('P') => Some(Action::ApprovalRespond(ApprovalDecision::AcceptPermanent)),
                _ => Some(Action::CancelSuggestions),
            };
        }

        match active_mode {
            AppMode::Commands
            | AppMode::Tree
            | AppMode::Sessions
            | AppMode::Settings
            | AppMode::Models => {
                if let KeyCode::Char(ch) = key.code {
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                    {
                        return Some(Action::FilterAppend(ch));
                    }
                }
                if let KeyCode::Backspace = key.code {
                    return Some(Action::FilterBackspace);
                }
                return match ka {
                    Some(KeyAction::SelectPrev) => Some(Action::SelectPrev),
                    Some(KeyAction::SelectNext) => Some(Action::SelectNext),
                    Some(KeyAction::Submit | KeyAction::Confirm) => Some(Action::ConfirmSelection),
                    Some(KeyAction::Cancel) => Some(Action::CloseSurface),
                    Some(KeyAction::Exit) => Some(Action::Quit),
                    None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
                    _ => None,
                };
            }
            AppMode::Status => {
                return match ka {
                    Some(KeyAction::SelectPrev) => Some(Action::SelectPrev),
                    Some(KeyAction::SelectNext) => Some(Action::SelectNext),
                    Some(KeyAction::Submit | KeyAction::Confirm) => Some(Action::ConfirmSelection),
                    Some(KeyAction::Cancel) => Some(Action::CloseSurface),
                    Some(KeyAction::Exit) => Some(Action::Quit),
                    None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
                    _ => None,
                };
            }
            AppMode::Help => {
                return match ka {
                    Some(KeyAction::Cancel | KeyAction::Submit | KeyAction::Confirm) => {
                        Some(Action::CloseSurface)
                    }
                    Some(KeyAction::Exit) => Some(Action::Quit),
                    None if matches!(key.code, KeyCode::Char('q')) => Some(Action::CloseSurface),
                    _ => None,
                };
            }
            AppMode::Chat | AppMode::Approval => {}
        }

        // 3. Chat / Editor Routing
        // Autocomplete (editor child handler) intercepts Up/Down/Tab/Enter
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

        // Standard editor inputs, approvals & timeline scroll fallbacks
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
