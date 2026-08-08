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
