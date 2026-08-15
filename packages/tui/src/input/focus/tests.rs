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
    let app = app();
    let keymap = Keymap::default();
    let action = InputRouter::route_key(
        &app,
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

#[test]
fn notification_panel_routes_selection_and_copy_keys() {
    let mut app = app();
    app.notifications.open_modal();
    app.push_surface(crate::app::SurfaceId::Notifications);
    let keymap = Keymap::default();
    let event = |code| KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };

    assert!(matches!(
        InputRouter::route_key(&app, &keymap, event(KeyCode::Down)),
        Some(Action::Notifications(NotificationAction::SelectNext))
    ));
    assert!(matches!(
        InputRouter::route_key(&app, &keymap, event(KeyCode::Char('c'))),
        Some(Action::Notifications(NotificationAction::CopySelected))
    ));
    assert!(matches!(
        InputRouter::route_key(&app, &keymap, event(KeyCode::Enter)),
        Some(Action::Notifications(NotificationAction::CopySelected))
    ));
}
