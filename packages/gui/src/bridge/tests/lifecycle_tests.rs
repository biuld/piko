use super::*;

#[test]
fn initial_phase_is_idle() {
    let bridge = headless();
    assert_eq!(bridge.state().session_phase, SessionPhase::IdleNoSession);
}

#[test]
fn connected_sets_connection_state() {
    let mut bridge = headless();
    bridge.mark_connected();
    assert_eq!(
        bridge.state().shell.connection,
        piko_client_core::state::ConnectionState::Connected,
    );
}

#[test]
fn open_session_transitions_to_opening() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });

    match &bridge.state().session_phase {
        SessionPhase::OpeningOrCreating { target_id } => {
            assert_eq!(target_id.as_deref(), Some("s1"));
        }
        other => panic!("expected OpeningOrCreating, got {other:?}"),
    }
}

#[test]
fn open_then_identity_result_transitions_to_hydrating() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });

    let cmd_id = extract_command_id(&mut bridge);

    bridge.apply_transport_event(crate::transport::TransportEvent::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Ok(piko_protocol::CommandResult::SessionOpened {
                session_id: "s1".into(),
                timestamp: 0,
            }),
        },
    )));

    match &bridge.state().session_phase {
        SessionPhase::Hydrating { target_id } => {
            assert_eq!(target_id, "s1");
        }
        other => panic!("expected Hydrating, got {other:?}"),
    }
}

#[test]
fn reconcile_transitions_to_live() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });

    let cmd_id = extract_command_id(&mut bridge);
    bridge.apply_transport_event(crate::transport::TransportEvent::Message(Box::new(
        piko_protocol::ServerMessage::CommandResponse {
            command_id: cmd_id,
            result: Ok(piko_protocol::CommandResult::SessionOpened {
                session_id: "s1".into(),
                timestamp: 0,
            }),
        },
    )));

    bridge.apply(ClientMsg::Host(Box::new(
        piko_protocol::ServerMessage::SessionReconciled(minimal_reconciled("s1")),
    )));

    assert_eq!(bridge.state().session_phase, SessionPhase::Live);
    assert!(bridge.state().live_session.is_some());
}

// ─── Transport closed must not fabricate session ─────────────────────────────

#[test]
fn transport_closed_does_not_fabricate_session() {
    let mut bridge = headless();
    bridge.mark_connected();

    bridge.apply(ClientMsg::Transport(TransportObservation::Closed));

    assert_eq!(
        bridge.state().shell.connection,
        piko_client_core::state::ConnectionState::Disconnected,
    );
    assert_eq!(bridge.state().session_phase, SessionPhase::IdleNoSession);
    assert!(bridge.state().live_session.is_none());
}

#[test]
fn transport_closed_during_hydrating_keeps_idle() {
    let mut bridge = headless();
    bridge.mark_connected();
    bridge.intent(ClientIntent::OpenSession {
        session_id: "s1".into(),
        session_path: None,
    });

    bridge.apply(ClientMsg::Transport(TransportObservation::Closed));

    assert!(bridge.state().live_session.is_none());
    assert_eq!(
        bridge.state().shell.connection,
        piko_client_core::state::ConnectionState::Disconnected,
    );
}

// ─── DiscoverSessions produces Send effect ───────────────────────────────────
