use super::*;

#[test]
fn diff_slash_fetches_turn_diff_when_last_turn_known() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.last_turn_id = Some("turn-9".into());

    let effects = app.try_slash_command("/diff").expect("known slash");
    assert!(effects.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::TurnDiffGet { turn_id, .. })
                if turn_id == "turn-9"
        )
    }));
}

#[test]
fn turn_diff_got_opens_diagnostics_panel() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        command_id: "c1".into(),
        result: Ok(piko_protocol::CommandResult::TurnDiffGot {
            diff: Some(piko_protocol::TurnDiffEvent {
                session_id: "session-1".into(),
                turn_id: "turn-9".into(),
                files: vec![piko_protocol::TurnFileChange {
                    path: "src/main.rs".into(),
                    before: None,
                    after: None,
                }],
                unified_diff: "+fn main() {}\n".into(),
            }),
            timestamp: 0,
        }),
    });
    assert_eq!(app.focus_manager.active_mode(), AppMode::Diagnostics);
    assert_eq!(app.last_turn_id.as_deref(), Some("turn-9"));
    assert!(app.last_turn_diff.is_some());
}
