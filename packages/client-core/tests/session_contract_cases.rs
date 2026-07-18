//! Contract baseline tests C1–C3 for piko-client-core.

mod helpers;

use helpers::*;
use piko_client_core::{ClientIntent, ClientState, SessionPhase};
use piko_protocol::{
    Command, CommandResult, ReconcileReason, ServerMessage, SessionListScope,
    SessionReconciledEvent, SessionSummary,
};

// ═══════════════════════════════════════════════════════════════════════════
// C1 — No-session startup and SessionList
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c1_discover_sessions_emits_session_list() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, effects) = intent(
        state,
        ClientIntent::DiscoverSessions {
            scope: SessionListScope::All,
            cwd: None,
        },
        &mut ids,
    );

    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::SessionList {
            command_id,
            scope,
            cwd,
        } => {
            assert_eq!(command_id, "cmd-1");
            assert_eq!(*scope, SessionListScope::All);
            assert!(cwd.is_none());
        }
        _ => panic!("expected SessionList"),
    }
    assert_eq!(state.session_phase, SessionPhase::IdleNoSession);
}

#[test]
fn c1_session_listed_populates_projection() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, _) = intent(
        state,
        ClientIntent::DiscoverSessions {
            scope: SessionListScope::All,
            cwd: None,
        },
        &mut ids,
    );

    let summaries = vec![SessionSummary {
        session_id: "s1".into(),
        cwd: "/tmp".into(),
        seq: 1,
        name: Some("Test".into()),
        first_message: None,
        message_count: 0,
        created_at: None,
        modified_at: None,
        session_path: None,
        parent_session_path: None,
    }];

    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionListed {
                sessions: summaries,
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::IdleNoSession);
    assert_eq!(state.session_list.sessions.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// C2 — Open Session to Live
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c2_open_transitions_to_opening() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, effects) = intent(
        state,
        ClientIntent::OpenSession {
            session_id: "sess-1".into(),
            session_path: None,
        },
        &mut ids,
    );

    assert_eq!(
        state.session_phase,
        SessionPhase::OpeningOrCreating {
            target_id: Some("sess-1".into())
        }
    );
    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::SessionOpen { session_id, .. } => assert_eq!(session_id, "sess-1"),
        _ => panic!("expected SessionOpen"),
    }
}

#[test]
fn c2_session_opened_transitions_to_hydrating() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, _) = intent(
        state,
        ClientIntent::OpenSession {
            session_id: "sess-1".into(),
            session_path: None,
        },
        &mut ids,
    );

    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionOpened {
                session_id: "sess-1".into(),
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    assert_eq!(
        state.session_phase,
        SessionPhase::Hydrating {
            target_id: "sess-1".into()
        }
    );
    assert!(state.live_session.is_none());
}

#[test]
fn c2_reconcile_enters_live() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    assert_eq!(state.session_phase, SessionPhase::Live);
    let live = state.live_session.as_ref().unwrap();
    assert_eq!(live.session_id, "sess-1");
    assert_eq!(live.selected_agent, Some("root".to_string()));
}

#[test]
fn c2_reconcile_hydrates_timelines_from_active_branch() {
    let mut ids = SeqIds(0);
    let (state, _) = intent(
        ClientState::default(),
        ClientIntent::OpenSession {
            session_id: "sess-1".into(),
            session_path: None,
        },
        &mut ids,
    );
    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionOpened {
                session_id: "sess-1".into(),
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    let mut snapshot = session_snapshot("sess-1");
    snapshot.current_leaf_id = Some("msg-1".into());
    snapshot.entries = vec![piko_protocol::session::SessionTreeEntry::Message(
        piko_protocol::session::MessageEntry {
            id: "msg-1".into(),
            parent_id: None,
            timestamp: "1".into(),
            agent_id: "main".into(),
            agent_instance_id: "root".into(),
            source_turn_id: "turn-1".into(),
            transcript_seq: 1,
            message: piko_protocol::Message::User {
                content: piko_protocol::MessageContent::String("hello from tree".into()),
                timestamp: Some(1),
            },
        },
    )];

    let (state, _) = host(
        state,
        ServerMessage::SessionReconciled(SessionReconciledEvent {
            session_id: "sess-1".into(),
            reason: ReconcileReason::InitialHydration,
            cursor: piko_protocol::agent_runtime::SessionCursor {
                epoch: "e1".into(),
                seq: 1,
            },
            snapshot,
            agents: vec![agent_info("sess-1", "root", None)],
        }),
        &mut ids,
    );

    let tl = state
        .live_session
        .as_ref()
        .unwrap()
        .timelines
        .get("root")
        .expect("root timeline from reconcile");
    assert_eq!(tl.committed_count(), 1);
}

#[test]
fn c2_foreign_reconcile_ignored() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    // Open sess-1
    let (state, _) = intent(
        state,
        ClientIntent::OpenSession {
            session_id: "sess-1".into(),
            session_path: None,
        },
        &mut ids,
    );
    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionOpened {
                session_id: "sess-1".into(),
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    // Foreign reconcile for sess-2 should be ignored
    let (state, _) = host(
        state,
        reconcile_event("sess-2", ReconcileReason::InitialHydration),
        &mut ids,
    );

    assert_eq!(
        state.session_phase,
        SessionPhase::Hydrating {
            target_id: "sess-1".into()
        }
    );
    assert!(state.live_session.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// C3 — Create Session to Live
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c3_create_session_to_live() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, effects) = intent(
        state,
        ClientIntent::CreateSession {
            cwd: "/home/user".into(),
        },
        &mut ids,
    );

    assert_eq!(
        state.session_phase,
        SessionPhase::OpeningOrCreating { target_id: None }
    );
    match first_command(&effects) {
        Command::SessionCreate { cwd, .. } => assert_eq!(cwd, "/home/user"),
        _ => panic!("expected SessionCreate"),
    }

    // SessionCreated response
    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionCreated {
                session_id: "new-sess".into(),
                cwd: "/home/user".into(),
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    assert_eq!(
        state.session_phase,
        SessionPhase::Hydrating {
            target_id: "new-sess".into()
        }
    );

    // Reconcile
    let (state, _) = host(
        state,
        reconcile_event("new-sess", ReconcileReason::InitialHydration),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::Live);
    assert_eq!(state.live_session.as_ref().unwrap().session_id, "new-sess");
}

// ═══════════════════════════════════════════════════════════════════════════
