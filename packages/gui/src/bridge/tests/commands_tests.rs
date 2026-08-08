use super::*;

#[test]
fn discover_sessions_sends_session_list_command() {
    let mut bridge = headless();
    bridge.mark_connected();

    bridge.intent(ClientIntent::DiscoverSessions {
        scope: SessionListScope::CurrentFolder,
        cwd: Some("/projects/foo".into()),
    });

    let sent = bridge.take_sent();
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        piko_protocol::Command::SessionList { scope, cwd, .. } => {
            assert_eq!(*scope, SessionListScope::CurrentFolder);
            assert_eq!(cwd.as_deref(), Some("/projects/foo"));
        }
        other => panic!("expected SessionList, got {other:?}"),
    }
}

#[test]
fn create_session_transitions_and_sends() {
    let mut bridge = headless();
    bridge.mark_connected();

    bridge.intent(ClientIntent::CreateSession {
        cwd: "/tmp/proj".into(),
    });

    match &bridge.state().session_phase {
        SessionPhase::OpeningOrCreating { target_id } => {
            assert!(target_id.is_none());
        }
        other => panic!("expected OpeningOrCreating, got {other:?}"),
    }

    let sent = bridge.take_sent();
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        piko_protocol::Command::SessionCreate { cwd, .. } => {
            assert_eq!(cwd, "/tmp/proj");
        }
        other => panic!("expected SessionCreate, got {other:?}"),
    }
}

#[test]
fn approval_requested_then_respond_keeps_until_resolved() {
    use piko_client_core::{AttentionKind, prompt_queue};
    use piko_protocol::{ApprovalDecision, ApprovalEvent};

    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });
    let cmd_id = extract_command_id(&mut bridge);
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Ok(piko_protocol::CommandResult::SessionOpened {
                session_id: "s1".into(),
                timestamp: 1,
            }),
        },
    )));
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::SessionReconciled(minimal_reconciled("s1")),
    )));
    assert_eq!(bridge.state().session_phase, SessionPhase::Live);

    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::Approval(ApprovalEvent::Requested {
            session_id: "s1".into(),
            agent_instance_id: "agent-root".into(),
            agent_id: "main".into(),
            approval_id: "ap-1".into(),
            tool_name: "exec".into(),
            tool_args: serde_json::json!({}),
            prompt: None,
        }),
    )));

    let session = bridge.state().live_session.as_ref().unwrap();
    let q = prompt_queue(session);
    assert_eq!(q.len(), 1);
    assert_eq!(q[0].kind, AttentionKind::Approval);

    bridge.intent(ClientIntent::RespondApproval {
        approval_id: "ap-1".into(),
        decision: ApprovalDecision::Accept,
        note: None,
    });
    let sent = bridge.take_sent();
    assert!(sent.iter().any(|c| matches!(
        c,
        piko_protocol::Command::ApprovalRespond { approval_id, .. } if approval_id == "ap-1"
    )));
    assert!(
        bridge
            .state()
            .live_session
            .as_ref()
            .unwrap()
            .pending_approvals[0]
            .response_in_flight
    );

    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::Approval(ApprovalEvent::Resolved {
            session_id: "s1".into(),
            approval_id: "ap-1".into(),
            decision: ApprovalDecision::Accept,
        }),
    )));
    assert!(
        bridge
            .state()
            .live_session
            .as_ref()
            .unwrap()
            .pending_approvals
            .is_empty()
    );
}

#[test]
fn navigate_session_emits_session_navigate() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });
    let cmd_id = extract_command_id(&mut bridge);
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Ok(piko_protocol::CommandResult::SessionOpened {
                session_id: "s1".into(),
                timestamp: 1,
            }),
        },
    )));
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::SessionReconciled(minimal_reconciled("s1")),
    )));

    bridge.take_sent();
    bridge.intent(ClientIntent::NavigateSession {
        entry_id: "entry-9".into(),
        summarize: true,
        custom_instructions: None,
    });
    let sent = bridge.take_sent();
    assert!(sent.iter().any(|c| matches!(
        c,
        piko_protocol::Command::SessionNavigate {
            entry_id,
            summarize: true,
            ..
        } if entry_id == "entry-9"
    )));
}

#[test]
fn decode_failure_then_valid_host_message() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.apply(ClientMsg::Transport(TransportObservation::DecodeFailure {
        detail: "bad line".into(),
    }));
    assert!(bridge.state().last_error.is_some());

    bridge.intent(ClientIntent::DiscoverSessions {
        scope: SessionListScope::All,
        cwd: None,
    });
    let cmd_id = extract_command_id(&mut bridge);
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Ok(piko_protocol::CommandResult::SessionListed {
                sessions: vec![],
                timestamp: 1,
            }),
        },
    )));
    assert!(bridge.state().pending_commands.is_empty());
    assert_eq!(
        bridge.state().shell.connection,
        piko_client_core::state::ConnectionState::Connected
    );
}

#[test]
fn closed_clears_pending_open_without_fabricating_live() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });
    assert!(!bridge.state().pending_commands.is_empty());
    bridge.apply(ClientMsg::Transport(TransportObservation::Closed));
    assert!(bridge.state().pending_commands.is_empty());
    assert!(bridge.state().live_session.is_none());
    assert_eq!(
        bridge.state().shell.connection,
        piko_client_core::state::ConnectionState::Disconnected,
    );
}

#[test]
fn rejected_open_surfaces_error_without_live_session() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "missing".into(),
        session_path: None,
    });
    let cmd_id = extract_command_id(&mut bridge);
    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Err("session not found".into()),
        },
    )));
    assert_eq!(
        bridge.state().last_error.as_deref(),
        Some("session not found")
    );
    assert!(bridge.state().live_session.is_none());
    assert_eq!(bridge.state().session_phase, SessionPhase::IdleNoSession);
}

#[test]
fn gui_config_commands_stay_in_frontend_bridge() {
    let mut bridge = headless();
    bridge.request_gui_config();
    assert!(matches!(
        bridge.take_sent().as_slice(),
        [piko_protocol::Command::ConfigGet { namespace, .. }] if namespace == "gui"
    ));

    bridge.apply_transport_event(crate::transport::TransportEvent::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: "config-get".into(),
            result: Ok(piko_protocol::CommandResult::ConfigEntry {
                namespace: "gui".into(),
                value: serde_json::json!({"session-open": false}),
            }),
        },
    )));
    assert_eq!(bridge.gui_config().unwrap()["session-open"], false);
    assert!(bridge.state().live_session.is_none());

    bridge.update_gui_config(serde_json::json!({"session-open": true}));
    assert!(matches!(
        bridge.take_sent().as_slice(),
        [piko_protocol::Command::ConfigUpdate { patch, .. }]
            if patch["gui"]["session-open"] == true
    ));
}

#[test]
fn host_config_get_and_update_use_host_fields() {
    let mut bridge = headless();
    bridge.request_host_config();
    assert!(matches!(
        bridge.take_sent().as_slice(),
        [piko_protocol::Command::ConfigGet { namespace, .. }] if namespace == "host"
    ));

    bridge.apply_transport_event(crate::transport::TransportEvent::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: "host-get".into(),
            result: Ok(piko_protocol::CommandResult::ConfigEntry {
                namespace: "host".into(),
                value: serde_json::json!({
                    "sandbox": { "enabled": false }
                }),
            }),
        },
    )));
    assert_eq!(bridge.host_config().unwrap()["sandbox"]["enabled"], false);

    bridge.update_host_config(serde_json::json!({ "retry": { "enabled": false } }));
    assert!(matches!(
        bridge.take_sent().as_slice(),
        [piko_protocol::Command::ConfigUpdate { patch, .. }]
            if patch["retry"]["enabled"] == false && patch.get("gui").is_none()
    ));
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
