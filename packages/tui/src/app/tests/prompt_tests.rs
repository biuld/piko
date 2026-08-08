use super::*;

#[test]
fn prompt_debug_slash_sends_command() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    let effects = app.try_slash_command("/prompt-debug").expect("known slash");
    assert!(effects.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::PromptDebugGet {
                agent_instance_id,
                ..
            }) if agent_instance_id == "agent-1"
        )
    }));
}

#[test]
fn rollout_slash_sends_command() {
    let mut app = live_app();
    with_local_slash_catalog(&mut app);
    app.agent_panel.active_agent_instance_id = Some("agent-1".into());
    let effects = app.try_slash_command("/rollout").expect("known slash");
    assert!(effects.iter().any(|e| {
        matches!(
            e,
            Effect::Send(piko_protocol::Command::RolloutPageGet {
                agent_instance_id,
                ..
            }) if agent_instance_id == "agent-1"
        )
    }));
}
