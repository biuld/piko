//! Shared fixture builders for Client Core contract tests.

#![allow(dead_code)] // helpers are shared across integration test crates

use piko_client_core::{
    ClientEffect, ClientIntent, ClientMsg, ClientState, CommandIdSource, SessionPhase,
    UpdateContext, update,
};
use piko_protocol::agent_runtime::SessionCursor;
use piko_protocol::{
    AgentInfo, CommandResult, ReconcileReason, ServerMessage, SessionReconciledEvent,
    SessionSnapshot,
};

/// Deterministic command id source for tests.
pub struct SeqIds(pub u64);

impl CommandIdSource for SeqIds {
    fn next_command_id(&mut self) -> String {
        self.0 += 1;
        format!("cmd-{}", self.0)
    }
}

/// Shorthand to apply a message with deterministic ids.
pub fn apply(
    state: ClientState,
    msg: ClientMsg,
    ids: &mut SeqIds,
) -> (ClientState, Vec<ClientEffect>) {
    let mut ctx = UpdateContext { command_ids: ids };
    update(state, msg, &mut ctx)
}

pub fn intent(
    state: ClientState,
    intent: ClientIntent,
    ids: &mut SeqIds,
) -> (ClientState, Vec<ClientEffect>) {
    apply(state, ClientMsg::Intent(intent), ids)
}

pub fn host(
    state: ClientState,
    msg: ServerMessage,
    ids: &mut SeqIds,
) -> (ClientState, Vec<ClientEffect>) {
    apply(state, ClientMsg::Host(Box::new(msg)), ids)
}

/// Build a minimal AgentInfo.
pub fn agent_info(session_id: &str, instance_id: &str, parent: Option<&str>) -> AgentInfo {
    AgentInfo {
        session_id: session_id.to_string(),
        agent_instance_id: instance_id.to_string(),
        agent_id: format!("{instance_id}-spec"),
        parent_agent_instance_id: parent.map(|s| s.to_string()),
        lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
        activity: piko_protocol::AgentActivity::Idle,
        unread_report_count: 0,
        name: format!("Agent {instance_id}"),
        role: "assistant".to_string(),
        status: piko_protocol::AgentStatus::Idle,
    }
}

/// Build a minimal SessionSnapshot.
pub fn session_snapshot(session_id: &str) -> SessionSnapshot {
    SessionSnapshot {
        session_id: session_id.to_string(),
        cwd: "/tmp/test".to_string(),
        seq: 1,
        entries: Vec::new(),
        current_leaf_id: None,
        selected_agent_instance_id: Some("root".to_string()),
        active_turns: Vec::new(),
        pending_approvals: Vec::new(),
        pending_interactions: Vec::new(),
        name: Some("Test Session".to_string()),
        cumulative_usage: None,
    }
}

/// Build a SessionReconciled event.
pub fn reconcile_event(session_id: &str, reason: ReconcileReason) -> ServerMessage {
    ServerMessage::SessionReconciled(SessionReconciledEvent {
        session_id: session_id.to_string(),
        reason,
        cursor: SessionCursor {
            epoch: "e1".to_string(),
            seq: 1,
        },
        snapshot: session_snapshot(session_id),
        agents: vec![agent_info(session_id, "root", None)],
    })
}

/// Build a CommandResponse with Ok result.
pub fn cmd_ok(command_id: &str, result: CommandResult) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Ok(result),
    }
}

/// Build a CommandResponse with Err result.
pub fn cmd_err(command_id: &str, msg: &str) -> ServerMessage {
    ServerMessage::CommandResponse {
        command_id: command_id.to_string(),
        result: Err(msg.to_string()),
    }
}

/// Extract the first Send effect's command, panic if none.
pub fn first_command(effects: &[ClientEffect]) -> &piko_protocol::Command {
    match effects.first().expect("expected at least one effect") {
        ClientEffect::Send(cmd) => cmd,
    }
}

/// Drive a session to Live phase starting from default state.
pub fn drive_to_live(ids: &mut SeqIds, session_id: &str) -> ClientState {
    let state = ClientState::default();
    // Open
    let (state, _) = intent(
        state,
        ClientIntent::OpenSession {
            session_id: session_id.to_string(),
            session_path: None,
        },
        ids,
    );
    // SessionOpened response
    let (state, _) = host(
        state,
        cmd_ok(
            "cmd-1",
            CommandResult::SessionOpened {
                session_id: session_id.to_string(),
                timestamp: 1,
            },
        ),
        ids,
    );
    // Reconcile
    let (state, _) = host(
        state,
        reconcile_event(session_id, ReconcileReason::InitialHydration),
        ids,
    );
    assert_eq!(state.session_phase, SessionPhase::Live);
    state
}
