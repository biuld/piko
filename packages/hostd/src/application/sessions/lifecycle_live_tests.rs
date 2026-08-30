use crate::api::ServerMessage;
use crate::application::host_app::HostApp;
use piko_orchd_api::AgentCommitPort;
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
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput::from_request(
                        &piko_protocol::SendAgentInputRequest {
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
                        2,
                    ),
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 2,
                },
            },
        )
        .await
        .unwrap();

    let events = HostApp::session_open_response(
        &mut state,
        "open-queued",
        session_id.clone(),
        Some(temp.path()),
        &crate::adapters::storage::FsSessionStoreFactory,
        false,
    )
    .await
    .unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, ServerMessage::SessionReconciled(_)))
    );
}

#[tokio::test]
async fn same_process_open_preserves_live_turn_for_reconcile() {
    let mut state = crate::domain::sessions::HostState::new();
    let crate::api::CommandResult::SessionCreated { session_id, .. } =
        state.create_session("/project")
    else {
        unreachable!()
    };
    let factory = crate::adapters::storage::FsSessionStoreFactory;

    let events = HostApp::session_open_response(
        &mut state,
        "open-live",
        session_id.clone(),
        None,
        &factory,
        true,
    )
    .await
    .unwrap();

    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::SessionReconciled(reconciled)
            if reconciled.snapshot.agent_work.is_empty()
    )));
}
