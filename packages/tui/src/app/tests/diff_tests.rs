use super::*;

#[test]
fn diff_slash_fetches_agent_work_diff_when_last_turn_known() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    app.last_root_input_id = Some("turn-9".into());

    let effects = app.try_slash_command("/diff").expect("known slash");
    assert!(effects.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::AgentWorkDiffGet { root_input_id, .. })
                if root_input_id == "turn-9"
        )
    }));
}

#[test]
fn agent_work_diff_got_opens_diagnostics_panel() {
    let mut app = live_app();
    app.apply_event(Event::CommandResponse {
        command_id: "c1".into(),
        result: Ok(piko_protocol::CommandResult::AgentWorkDiffGot {
            diff: Some(piko_protocol::AgentWorkDiffEvent {
                session_id: "session-1".into(),
                root_input_id: "turn-9".into(),
                files: vec![piko_protocol::AgentWorkFileChange {
                    path: "src/main.rs".into(),
                    before: None,
                    after: None,
                }],
                unified_diff: "+fn main() {}\n".into(),
            }),
            timestamp: 0,
        }),
    });
    assert_eq!(
        app.focus_manager.active_mode(),
        AppMode::Surface(SurfaceId::Diagnostics)
    );
    assert_eq!(app.last_root_input_id.as_deref(), Some("turn-9"));
    assert!(app.last_agent_work_diff.is_some());
}
