use crate::api::ServerMessage;
use crate::application::host_app::HostApp;
use orchd_api::AgentCommitPort;
use piko_protocol::AgentDurableCommand;

#[tokio::test]
async fn session_open_restores_queued_turn_from_durable_agent_input() {
    let mut state = crate::domain::sessions::HostState::new();
    let crate::api::CommandResult::SessionCreated { session_id, .. } =
        state.create_session("/project")
    else {
        unreachable!()
    };
    let temp = tempfile::tempdir().unwrap();
    let store = crate::infra::storage::SessionStore::create_session(
        temp.path(),
        session_id.clone(),
        "/project".into(),
        1,
    )
    .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::InputQueued {
                agent_instance_id: root.agent_instance_id.clone(),
                queued_input: piko_protocol::DurableAgentInput {
                    queued_input_id: "turn-queued".into(),
                    request: piko_protocol::SendAgentInputRequest {
                        request_id: "turn-queued".into(),
                        session_id: session_id.clone(),
                        agent_instance_id: root.agent_instance_id,
                        caller_agent_instance_id: None,
                        source_turn_id: Some("turn-queued".into()),
                        message_id: "message-queued".into(),
                        content: piko_protocol::MessageContent::String("follow up".into()),
                        delivery: piko_protocol::AgentInputDelivery::FollowUp,
                        prompt_resources: None,
                        active_tool_names: None,
                    },
                    detached_recipient_agent_instance_id: None,
                },
            },
        )
        .await
        .unwrap();

    HostApp::session_open_response(
        &mut state,
        "open-queued",
        session_id.clone(),
        Some(temp.path()),
        &crate::adapters::storage::FsSessionStoreFactory,
        false,
    )
    .unwrap();

    let turn = state.turn(&session_id, "turn-queued").unwrap();
    assert_eq!(turn.status, crate::api::TurnStatus::Queued);
    assert_eq!(turn.message, "follow up");
}

#[test]
fn same_process_open_preserves_live_turn_for_reconcile() {
    let mut state = crate::domain::sessions::HostState::new();
    let crate::api::CommandResult::SessionCreated { session_id, .. } =
        state.create_session("/project")
    else {
        unreachable!()
    };
    let root_agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &root_agent_instance_id, "keep running")
        .unwrap();
    state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let factory = crate::adapters::storage::FsSessionStoreFactory;

    let events = HostApp::session_open_response(
        &mut state,
        "open-live",
        session_id.clone(),
        None,
        &factory,
        true,
    )
    .unwrap();

    assert_eq!(
        state
            .session(&session_id)
            .unwrap()
            .active_turns
            .get(&root_agent_instance_id)
            .map(String::as_str),
        Some(turn_id.as_str())
    );
    assert!(events.iter().all(|event| !matches!(
        event,
        ServerMessage::TurnLifecycle(crate::api::TurnEvent::Failed { .. })
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::SessionReconciled(reconciled)
            if reconciled.snapshot.active_turns.iter().any(|turn| turn.turn_id == turn_id)
    )));
}
