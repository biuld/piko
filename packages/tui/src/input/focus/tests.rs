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

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn follow_up_steer_and_dequeue_keys_reach_the_editor() {
    let app = app();
    let keymap = Keymap::default();

    assert!(matches!(
        InputRouter::route_key(&app, &keymap, key(KeyCode::Enter, KeyModifiers::ALT)),
        Some(Action::Editor(EditorAction::FollowUp))
    ));
    assert!(matches!(
        InputRouter::route_key(&app, &keymap, key(KeyCode::Enter, KeyModifiers::CONTROL)),
        Some(Action::Editor(EditorAction::Steer))
    ));
    assert!(matches!(
        InputRouter::route_key(&app, &keymap, key(KeyCode::Up, KeyModifiers::ALT)),
        Some(Action::Editor(EditorAction::DequeueFollowUp))
    ));
}

#[test]
fn shift_enter_inserts_a_newline() {
    let app = app();
    let keymap = Keymap::default();

    let action = InputRouter::route_key(&app, &keymap, key(KeyCode::Enter, KeyModifiers::SHIFT));

    assert!(matches!(
        action,
        Some(Action::Editor(EditorAction::InsertNewline))
    ));
}

#[test]
fn shift_enter_is_disabled_when_multiline_is_off() {
    let mut app = app();
    app.tui_config.editor.multiline = false;
    let keymap = Keymap::default();

    assert!(
        InputRouter::route_key(&app, &keymap, key(KeyCode::Enter, KeyModifiers::SHIFT),).is_none()
    );
}

#[test]
fn ctrl_j_has_no_default_binding() {
    let app = app();
    let keymap = Keymap::default();

    assert!(
        InputRouter::route_key(
            &app,
            &keymap,
            key(KeyCode::Char('j'), KeyModifiers::CONTROL),
        )
        .is_none()
    );
}

#[test]
fn ctrl_a_moves_cursor_to_line_start() {
    let mut app = app();
    app.editor.restore_text("hello");
    let keymap = Keymap::default();

    let action = InputRouter::route_key(
        &app,
        &keymap,
        key(KeyCode::Char('a'), KeyModifiers::CONTROL),
    );

    assert!(matches!(
        action,
        Some(Action::Editor(EditorAction::CursorLineStart))
    ));
}

#[test]
fn ctrl_e_moves_cursor_to_line_end_when_not_browsing_history() {
    let mut app = app();
    app.editor.restore_text("hello");
    let keymap = Keymap::default();

    let action = InputRouter::route_key(
        &app,
        &keymap,
        key(KeyCode::Char('e'), KeyModifiers::CONTROL),
    );

    assert!(matches!(
        action,
        Some(Action::Editor(EditorAction::CursorLineEnd))
    ));
}

#[test]
fn ctrl_e_continues_history_browse_when_active() {
    let mut app = app();
    // Submit "hello" so it lands in history, then start browsing with Ctrl+P.
    app.editor.restore_text("hello");
    app.dispatch(crate::app::command::EditorAction::Submit.into());
    app.editor.restore_text("draft");
    app.dispatch(crate::app::command::EditorAction::HistoryPrev.into());
    assert!(app.editor.is_browsing_history());

    let keymap = Keymap::default();
    let action = InputRouter::route_key(
        &app,
        &keymap,
        key(KeyCode::Char('e'), KeyModifiers::CONTROL),
    );

    assert!(matches!(
        action,
        Some(Action::Editor(EditorAction::HistoryNext))
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

#[test]
fn non_text_surface_does_not_accept_bracketed_paste() {
    let mut app = app();
    app.push_surface(crate::app::SurfaceId::Models);
    assert!(!app.accepts_text_paste());
    app.clear_focus();
    assert!(app.accepts_text_paste());
}
