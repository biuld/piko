use super::*;
use crate::app::{
    AppState, InitialOptions,
    command::{AppAction, TimelineAction},
};
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
    let keymap = BindingRegistry::default();
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
    let mut app = app();
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.session.active_turns.insert(
        "agent-1".into(),
        crate::app::ActiveTurnUi {
            turn_id: "turn-1".into(),
            status: piko_protocol::TurnStatus::Running,
        },
    );
    let keymap = BindingRegistry::default();

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
fn escape_interrupts_viewed_runtime_agent_without_a_host_turn() {
    let mut app = app();
    app.agent_panel.active_agent_instance_id = Some("agent-child".into());
    app.agent_panel
        .upsert_agent(crate::features::agent_status::AgentEntry {
            agent_id: "worker".into(),
            agent_instance_id: "agent-child".into(),
            name: "worker".into(),
            parent_agent_instance_id: Some("agent-root".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running,
            unread_report_count: 0,
            status: piko_protocol::AgentStatus::Running,
        });

    let action = InputRouter::route_key(
        &app,
        &BindingRegistry::default(),
        key(KeyCode::Esc, KeyModifiers::NONE),
    );

    assert!(matches!(
        action,
        Some(Action::Editor(EditorAction::Interrupt))
    ));
}

#[test]
fn shift_enter_inserts_a_newline() {
    let app = app();
    let keymap = BindingRegistry::default();

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
    let keymap = BindingRegistry::default();

    assert!(
        InputRouter::route_key(&app, &keymap, key(KeyCode::Enter, KeyModifiers::SHIFT),).is_none()
    );
}

#[test]
fn ctrl_j_has_no_default_binding() {
    let app = app();
    let keymap = BindingRegistry::default();

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
fn ctrl_d_quits_instead_of_deleting_forward() {
    let mut app = app();
    app.editor.restore_text("hello");
    let action = InputRouter::route_key(
        &app,
        &BindingRegistry::default(),
        key(KeyCode::Char('d'), KeyModifiers::CONTROL),
    );
    assert!(matches!(action, Some(Action::App(AppAction::Quit))));
}

#[test]
fn ctrl_c_copies_an_active_timeline_selection_before_editor_clear() {
    let mut app = app();
    app.timeline_mut()
        .start_selection(crate::features::timeline::SelectionPoint { row: 0, col: 0 });
    app.timeline_mut()
        .update_selection(crate::features::timeline::SelectionPoint { row: 0, col: 1 });
    app.timeline_mut()
        .finish_selection(crate::features::timeline::SelectionPoint { row: 0, col: 1 });
    let action = InputRouter::route_key(
        &app,
        &BindingRegistry::default(),
        key(KeyCode::Char('c'), KeyModifiers::CONTROL),
    );
    assert!(matches!(
        action,
        Some(Action::Timeline(TimelineAction::CopySelection))
    ));
}

#[test]
fn ctrl_a_moves_cursor_to_line_start() {
    let mut app = app();
    app.editor.restore_text("hello");
    let keymap = BindingRegistry::default();

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
    let keymap = BindingRegistry::default();

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
fn ctrl_e_remains_line_end_while_history_browse_is_active() {
    let mut app = app();
    // Submit "hello" so it lands in history, then start browsing with Ctrl+P.
    app.editor.restore_text("hello");
    app.dispatch(crate::app::command::EditorAction::Submit.into());
    app.editor.restore_text("draft");
    app.dispatch(crate::app::command::EditorAction::HistoryPrev.into());
    assert!(app.editor.is_browsing_history());

    let keymap = BindingRegistry::default();
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
fn notification_panel_routes_selection_and_copy_keys() {
    let mut app = app();
    app.notifications.open_modal();
    app.push_surface(crate::app::SurfaceId::Notifications);
    let keymap = BindingRegistry::default();
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
