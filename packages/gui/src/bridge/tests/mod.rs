//! Bridge unit tests using a headless (no transport) bridge.
//!
//! Validates Core integration paths without spawning hostd or opening GPUI
//! windows.

use piko_client_core::{
    ClientIntent, ClientMsg, CommandIdSource, SessionPhase, TransportObservation,
};
use piko_protocol::SessionListScope;

use super::ClientBridge;

// ─── Deterministic id source ─────────────────────────────────────────────────

struct SeqIdSource(u64);

impl SeqIdSource {
    fn new() -> Self {
        Self(0)
    }
}

impl CommandIdSource for SeqIdSource {
    fn next_command_id(&mut self) -> String {
        self.0 += 1;
        format!("cmd-{}", self.0)
    }
}

fn headless() -> ClientBridge {
    ClientBridge::headless(Box::new(SeqIdSource::new()))
}

// ─── Phase mapping ───────────────────────────────────────────────────────────

mod commands_tests;
mod lifecycle_tests;

fn extract_command_id(bridge: &mut ClientBridge) -> String {
    let sent = bridge.take_sent();
    assert!(!sent.is_empty(), "expected at least one sent command");
    match &sent[0] {
        piko_protocol::Command::SessionOpen { command_id, .. }
        | piko_protocol::Command::SessionCreate { command_id, .. }
        | piko_protocol::Command::SessionList { command_id, .. } => command_id.clone(),
        other => panic!("unexpected command variant: {other:?}"),
    }
}

fn minimal_reconciled(session_id: &str) -> piko_protocol::SessionReconciledEvent {
    piko_protocol::SessionReconciledEvent {
        session_id: session_id.into(),
        reason: piko_protocol::ReconcileReason::InitialHydration,
        cursor: piko_protocol::agent_runtime::SessionCursor {
            epoch: "e1".into(),
            seq: 1,
        },
        snapshot: minimal_snapshot(session_id),
        agents: vec![minimal_agent("agent-root")],
    }
}

fn minimal_snapshot(session_id: &str) -> piko_protocol::SessionSnapshot {
    piko_protocol::SessionSnapshot {
        session_id: session_id.into(),
        cwd: "/tmp".into(),
        seq: 1,
        name: None,
        entries: vec![],
        current_leaf_id: None,
        selected_agent_instance_id: Some("agent-root".into()),
        active_turns: vec![],
        pending_approvals: vec![],
        pending_interactions: vec![],
        cumulative_usage: None,
    }
}

fn minimal_agent(id: &str) -> piko_protocol::AgentInfo {
    piko_protocol::AgentInfo {
        session_id: "s1".into(),
        agent_instance_id: id.into(),
        agent_id: "main".into(),
        name: "Root".into(),
        role: "main".into(),
        parent_agent_instance_id: None,
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Idle,
        unread_report_count: 0,
        status: piko_protocol::AgentStatus::Idle,
    }
}
