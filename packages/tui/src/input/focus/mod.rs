//! Focus manager re-export and input routing.
//!
//! Focus stack type is generic (`piko_tui_layout::FocusManager<T>`); product
//! instantiates it as `FocusManager<AppMode>`.
//!
//! Input routing is a pure adapter from normalized input to root actions.

use crate::{app::AppState, input::binding::BindingRegistry, terminal::NormalizedInput};

#[cfg(test)]
use crate::app::command::{Action, EditorAction, NotificationAction};
#[cfg(test)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub use crate::navigation::FocusManager;

pub struct InputRouter;

impl InputRouter {
    pub fn route_input(
        app: &AppState,
        registry: &BindingRegistry,
        input: NormalizedInput,
    ) -> Option<crate::app::command::Action> {
        match input {
            NormalizedInput::Key { .. } => Self::route_normalized_key(app, registry, input),
            NormalizedInput::Paste(text) if app.accepts_text_paste() => {
                Some(crate::app::command::EditorAction::InsertPaste(text).into())
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn route_key<I: Into<NormalizedInput>>(
        app: &AppState,
        registry: &BindingRegistry,
        key: I,
    ) -> Option<crate::app::command::Action> {
        Self::route_input(app, registry, key.into())
    }
}

mod router;
#[cfg(test)]
mod tests;
