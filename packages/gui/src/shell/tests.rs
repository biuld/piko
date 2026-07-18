//! Pure view-model derivation tests — no GPUI or transport required.

use piko_client_core::state::{ConnectionState, LiveSession, SessionListProjection};
use piko_client_core::{ClientState, SessionPhase};
use piko_protocol::SessionSummary;

use super::view_model::*;

fn default_state() -> ClientState {
    ClientState::default()
}

fn live_state(session_id: &str) -> ClientState {
    let mut state = default_state();
    state.shell.connection = ConnectionState::Connected;
    state.session_phase = SessionPhase::Live;
    state.live_session = Some(LiveSession {
        session_id: session_id.into(),
        cwd: "/projects/myapp".into(),
        ..Default::default()
    });
    state
}

// ─── Phase view ──────────────────────────────────────────────────────────────

#[test]
fn idle_phase_maps_to_idle() {
    let state = default_state();
    assert_eq!(derive_phase_view(&state), SessionPhaseView::IdleNoSession);
}

#[test]
fn opening_phase_maps_correctly() {
    let mut state = default_state();
    state.session_phase = SessionPhase::OpeningOrCreating {
        target_id: Some("s1".into()),
    };
    assert_eq!(
        derive_phase_view(&state),
        SessionPhaseView::Opening {
            target_id: Some("s1".into()),
        },
    );
}

#[test]
fn hydrating_phase_maps_correctly() {
    let mut state = default_state();
    state.session_phase = SessionPhase::Hydrating {
        target_id: "s1".into(),
    };
    assert_eq!(
        derive_phase_view(&state),
        SessionPhaseView::Hydrating {
            target_id: "s1".into(),
        },
    );
}

#[test]
fn live_phase_maps_correctly() {
    let state = live_state("s1");
    assert_eq!(derive_phase_view(&state), SessionPhaseView::Live);
}

#[test]
fn error_during_non_live_maps_to_error() {
    let mut state = default_state();
    state.last_error = Some("connection failed".into());
    assert_eq!(
        derive_phase_view(&state),
        SessionPhaseView::Error {
            message: "connection failed".into(),
        },
    );
}

#[test]
fn error_during_live_still_shows_live() {
    let mut state = live_state("s1");
    state.last_error = Some("non-fatal warning".into());
    assert_eq!(derive_phase_view(&state), SessionPhaseView::Live);
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

#[test]
fn sidebar_rows_from_session_list() {
    let mut state = default_state();
    state.session_list = SessionListProjection {
        sessions: vec![
            stub_summary("s1", Some("Chat about Rust")),
            stub_summary("s2", None),
        ],
    };

    let vm = derive_sidebar(&state);
    assert_eq!(vm.rows.len(), 2);
    assert_eq!(vm.rows[0].session_id, "s1");
    assert_eq!(vm.rows[0].label, "Chat about Rust");
    assert_eq!(vm.rows[0].kind, SessionRowKind::Listed);
}

#[test]
fn sidebar_marks_live_session() {
    let mut state = live_state("s1");
    state.session_list = SessionListProjection {
        sessions: vec![stub_summary("s1", Some("My Session"))],
    };

    let vm = derive_sidebar(&state);
    assert_eq!(vm.rows[0].kind, SessionRowKind::LiveTarget);
}

#[test]
fn sidebar_marks_pending_target() {
    let mut state = default_state();
    state.session_phase = SessionPhase::Hydrating {
        target_id: "s2".into(),
    };
    state.session_list = SessionListProjection {
        sessions: vec![
            stub_summary("s1", Some("First")),
            stub_summary("s2", Some("Second")),
        ],
    };

    let vm = derive_sidebar(&state);
    let s2_row = vm.rows.iter().find(|r| r.session_id == "s2").unwrap();
    assert_eq!(s2_row.kind, SessionRowKind::PendingTarget);
}

// ─── Status bar ──────────────────────────────────────────────────────────────

#[test]
fn status_bar_disconnected() {
    let state = default_state();
    let vm = derive_status_bar(&state, false);
    assert_eq!(vm.connection, ConnectionStatus::Disconnected);
    assert!(vm.cwd.is_none());
    assert!(vm.usage.is_none());
}

#[test]
fn status_bar_hides_cwd_when_sidebar_visible() {
    let state = live_state("s1");
    let vm = derive_status_bar(&state, true);
    assert_eq!(vm.connection, ConnectionStatus::Connected);
    assert!(vm.cwd.is_none());
}

#[test]
fn status_bar_abbreviates_cwd_when_sidebar_hidden() {
    let state = live_state("s1");
    let vm = derive_status_bar(&state, false);
    assert_eq!(vm.connection, ConnectionStatus::Connected);
    assert_eq!(vm.cwd.as_deref(), Some("…/projects/myapp"));
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn stub_summary(id: &str, name: Option<&str>) -> SessionSummary {
    SessionSummary {
        session_id: id.into(),
        cwd: "/tmp".into(),
        seq: 1,
        name: name.map(String::from),
        first_message: None,
        message_count: 0,
        created_at: None,
        modified_at: None,
        session_path: None,
        parent_session_path: None,
    }
}
