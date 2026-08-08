use std::path::PathBuf;

use piko_protocol::{
    HostCommandDescriptor, HostCommandGroup, HostCommandInvoke, Message, ServerMessage as Event,
};
use serde_json::json;

use crate::app::{
    AppMode, AppState, InitialOptions, SurfaceId, ToolStatus, command::EditorAction,
    effect::Effect, get_active_branch_entries,
};
use crate::features::timeline::TimelineKind;

fn app() -> AppState {
    AppState::new(
        PathBuf::from("/tmp/piko-test"),
        None,
        false,
        InitialOptions::default(),
    )
}

fn live_app() -> AppState {
    let mut app = app();
    app.session.id = Some("session-1".into());
    app.session.shell_ready = true;
    app.agent_panel.mark_hydrated();
    app
}

fn empty_reconcile(session_id: &str) -> Event {
    Event::SessionReconciled(piko_protocol::SessionReconciledEvent {
        session_id: session_id.into(),
        reason: piko_protocol::ReconcileReason::InitialHydration,
        cursor: piko_protocol::agent_runtime::SessionCursor {
            epoch: format!("hostd:{session_id}"),
            seq: 0,
        },
        snapshot: piko_protocol::SessionSnapshot {
            session_id: session_id.into(),
            cwd: "/tmp/piko-test".into(),
            seq: 0,
            entries: Vec::new(),
            current_leaf_id: None,
            selected_agent_instance_id: Some(format!("agent_{session_id}_root")),
            active_turns: Vec::new(),
            pending_approvals: Vec::new(),
            pending_interactions: Vec::new(),
            name: None,
            cumulative_usage: None,
        },
        agents: vec![piko_protocol::AgentInfo {
            session_id: session_id.into(),
            agent_instance_id: format!("agent_{session_id}_root"),
            agent_id: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Idle,
            unread_report_count: 0,
            name: "Main".into(),
            role: "assistant".into(),
            status: piko_protocol::AgentStatus::Idle,
        }],
    })
}

fn realtime(
    message_id: &str,
    seq: u64,
    delta: piko_protocol::agent_runtime::RealtimeDelta,
) -> Event {
    Event::StreamItem(
        piko_protocol::StreamItemPatch::from_realtime_delta(
            Some("session-1".into()),
            Some("task-1".into()),
            message_id,
            Some(seq),
            &delta,
        )
        .into_iter()
        .next()
        .expect("realtime stream item"),
    )
}

fn committed(message_id: &str, task_seq: u64, message: Message) -> Event {
    Event::TranscriptCommitted(piko_protocol::TranscriptCommittedEvent {
        session_id: "session-1".into(),
        agent_instance_id: "task-1".into(),
        agent_id: "agent-1".into(),
        source_turn_id: "work-1".into(),
        message_id: message_id.into(),
        transcript_seq: task_seq,
        message,
    })
}

fn assistant(text: &str) -> Message {
    Message::Assistant {
        content: vec![piko_protocol::ContentBlock::Text { text: text.into() }],
        api: "test".into(),
        provider: "test".into(),
        model: "test".into(),
        usage: None,
        stop_reason: Some("stop".into()),
        error_message: None,
        timestamp: None,
    }
}

mod command_tests;
mod completion_tests;
mod delete_scope_tests;
mod diff_tests;
mod foreground_tests;
mod modal_tests;
mod pointer_tests;
mod prompt_tests;
mod session_tests;
mod snapshot_tests;
mod timeline_tests;
mod tree_tests;
mod usage_tests;

fn user_tree_entry(
    id: &str,
    parent_id: Option<&str>,
    text: &str,
) -> piko_protocol::SessionTreeEntry {
    piko_protocol::SessionTreeEntry::Message(piko_protocol::MessageEntry {
        id: id.into(),
        parent_id: parent_id.map(str::to_string),
        timestamp: "2026-06-29T12:00:00Z".into(),
        agent_id: "main".into(),
        agent_instance_id: "task-main".into(),
        source_turn_id: "work-main".into(),
        transcript_seq: 1,
        message: Message::User {
            content: piko_protocol::MessageContent::String(text.into()),
            timestamp: None,
        },
    })
}

fn with_local_slash_catalog(app: &mut AppState) {
    app.command_catalog = crate::app::command::merge_command_catalog(&[]);
}

/// `/resume` is a TUI-local command, always merged in regardless of what
/// hostd advertises; an empty host catalog is enough to exercise the
/// bootstrap round-trip this fixture supports.
fn test_command_catalog() -> Vec<HostCommandDescriptor> {
    Vec::new()
}
