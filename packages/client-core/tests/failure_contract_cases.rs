mod helpers;

use helpers::*;
use piko_client_core::{ClientIntent, ClientMsg, ClientState, SessionPhase, TransportObservation};
use piko_protocol::{Command, CommandResult, ServerMessage};

// C11 — Navigate / fork
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c11_navigate_session() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (_state, effects) = intent(
        state,
        ClientIntent::NavigateSession {
            entry_id: "entry-5".into(),
            summarize: true,
            custom_instructions: None,
        },
        &mut ids,
    );

    assert_eq!(effects.len(), 1);
    match first_command(&effects) {
        Command::SessionNavigate {
            session_id,
            entry_id,
            summarize,
            ..
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(entry_id, "entry-5");
            assert!(*summarize);
        }
        _ => panic!("expected SessionNavigate"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// C12 — Transport failure and malformed JSONL
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c12_connected_does_not_invent_session() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (state, _) = apply(
        state,
        ClientMsg::Transport(TransportObservation::Connected),
        &mut ids,
    );

    assert_eq!(state.session_phase, SessionPhase::IdleNoSession);
    assert!(state.live_session.is_none());
}

#[test]
fn c12_decode_failure_records_error() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = apply(
        state,
        ClientMsg::Transport(TransportObservation::DecodeFailure {
            detail: "invalid JSON".into(),
        }),
        &mut ids,
    );

    assert_eq!(state.last_error, Some("invalid JSON".to_string()));
    // Session still Live
    assert_eq!(state.session_phase, SessionPhase::Live);
}

#[test]
fn c12_closed_clears_pending_commands() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Issue a refresh so there's a pending command
    let (state, _) = intent(state, ClientIntent::RefreshSession, &mut ids);
    assert!(!state.pending_commands.is_empty());

    let (state, _) = apply(
        state,
        ClientMsg::Transport(TransportObservation::Closed),
        &mut ids,
    );

    assert!(state.pending_commands.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// C13 — Command error correlation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c13_open_error_restores_idle() {
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

    let (state, _) = host(state, cmd_err("cmd-1", "not found"), &mut ids);

    assert_eq!(state.session_phase, SessionPhase::IdleNoSession);
    assert_eq!(state.last_error, Some("not found".to_string()));
}

#[test]
fn c13_open_error_restores_previous_live() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    // Try to open another session
    let (state, _) = intent(
        state,
        ClientIntent::OpenSession {
            session_id: "sess-2".into(),
            session_path: None,
        },
        &mut ids,
    );

    // The live session is still there
    assert!(state.live_session.is_some());

    // Error on open: should restore to Live
    let (state, _) = host(state, cmd_err("cmd-2", "error"), &mut ids);

    assert_eq!(state.session_phase, SessionPhase::Live);
    assert_eq!(state.live_session.as_ref().unwrap().session_id, "sess-1");
}

#[test]
fn c13_submit_error_clears_pending_only() {
    let mut ids = SeqIds(0);
    let state = drive_to_live(&mut ids, "sess-1");

    let (state, _) = intent(
        state,
        ClientIntent::SubmitTurn { text: "hi".into() },
        &mut ids,
    );

    let (state, _) = host(state, cmd_err("cmd-2", "rejected"), &mut ids);

    // Session stays Live, just the error is recorded
    assert_eq!(state.session_phase, SessionPhase::Live);
    assert!(state.pending_commands.is_empty());
    assert_eq!(state.last_error, Some("rejected".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════════
// C3 — Model list / config update
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn c3_list_models_emits_model_list() {
    let mut ids = SeqIds(0);
    let state = ClientState::default();

    let (_state, effects) = intent(state, ClientIntent::ListModels, &mut ids);
    match first_command(&effects) {
        Command::ModelList { .. } => {}
        _ => panic!("expected ModelList"),
    }
}

#[test]
fn c3_model_listed_populates_catalog() {
    let mut ids = SeqIds(0);
    let (state, _) = intent(ClientState::default(), ClientIntent::ListModels, &mut ids);

    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::ModelListed {
                providers: vec![piko_protocol::ProviderInfo {
                    provider: "openai".into(),
                    has_auth: true,
                    models: vec![],
                }],
                timestamp: 1,
            },
        ),
        &mut ids,
    );

    assert_eq!(state.model.providers.len(), 1);
    assert_eq!(state.model.providers[0].provider, "openai");
}

#[test]
fn c3_set_model_emits_config_update() {
    let mut ids = SeqIds(0);
    let (state, effects) = intent(
        ClientState::default(),
        ClientIntent::SetModel {
            provider: "openai".into(),
            model_id: "gpt-4o".into(),
        },
        &mut ids,
    );

    match first_command(&effects) {
        Command::ConfigUpdate { patch, .. } => {
            assert_eq!(patch["default-provider"], "openai");
            assert_eq!(patch["default-model"], "gpt-4o");
        }
        _ => panic!("expected ConfigUpdate"),
    }
    assert!(matches!(
        state.pending_commands.get("cmd-1"),
        Some(piko_client_core::state::PendingOp::SetModel { provider, model_id })
            if provider == "openai" && model_id == "gpt-4o"
    ));
}

#[test]
fn c3_config_changed_updates_model_state() {
    let mut ids = SeqIds(0);
    let (state, _) = intent(
        ClientState::default(),
        ClientIntent::SetModel {
            provider: "openai".into(),
            model_id: "gpt-4o".into(),
        },
        &mut ids,
    );
    let (state, _) = host(
        state,
        ServerMessage::Model(piko_protocol::ModelEvent::ConfigChanged {
            model_id: "gpt-4o".into(),
            provider: "openai".into(),
            thinking_level: Some(piko_protocol::ThinkingLevel::High),
            timestamp: 1,
        }),
        &mut ids,
    );

    assert_eq!(state.model.model_id.as_deref(), Some("gpt-4o"));
    assert_eq!(state.model.provider.as_deref(), Some("openai"));
    assert_eq!(state.model.thinking_level.as_deref(), Some("high"));
    assert!(state.pending_commands.is_empty());
}
