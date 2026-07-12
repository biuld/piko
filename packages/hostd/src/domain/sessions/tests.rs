use super::*;
use crate::api::{RealtimeMessageEvent, ServerMessage};
use piko_protocol::agent_runtime::RealtimeDelta;

fn realtime(task_id: &str, agent_id: &str, message_id: &str, seq: u64) -> ServerMessage {
    ServerMessage::RealtimeMessage(RealtimeMessageEvent {
        session_id: "session".into(),
        agent_instance_id: task_id.into(),
        agent_id: agent_id.into(),
        message_id: message_id.into(),
        delta_seq: seq,
        delta: RealtimeDelta::MessageStarted {
            role: crate::api::MessageRole::Assistant,
        },
    })
}

#[test]
fn agent_view_store_records_task_views_and_replays_by_task() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp") {
        crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session created"),
    };

    state
        .append_agent_view_event(&session_id, "t1", "main", realtime("t1", "main", "m1", 0))
        .unwrap();
    state
        .append_agent_view_event(&session_id, "t2", "child", realtime("t2", "child", "m2", 0))
        .unwrap();
    state
        .append_agent_view_event(
            &session_id,
            "t1",
            "main",
            ServerMessage::RealtimeMessage(RealtimeMessageEvent {
                session_id: "session".into(),
                agent_instance_id: "t1".into(),
                agent_id: "main".into(),
                message_id: "m1".into(),
                delta_seq: 1,
                delta: RealtimeDelta::Text {
                    content_index: 0,
                    delta: "hello".into(),
                },
            }),
        )
        .unwrap();
    state
        .append_agent_view_event(&session_id, "t3", "main", realtime("t3", "main", "m3", 0))
        .unwrap();

    let main = state.agent_view_snapshot(&session_id, "t1").unwrap();
    assert_eq!(main.agent_id, "main");
    assert_eq!(main.agent_instance_id, "t1");
    assert_eq!(main.events.len(), 2);
    assert_eq!(main.next_seq, 4);

    let replay = state.agent_view_replay(&session_id, "t1", Some(1)).unwrap();
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0].seq, 3);

    let child = state.agent_view_snapshot(&session_id, "t2").unwrap();
    assert_eq!(child.agent_instance_id, "t2");
    assert_eq!(child.events.len(), 1);
    assert_eq!(child.events[0].seq, 2);
}

#[test]
fn agent_list_orders_parent_before_child_tasks() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp") {
        crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session created"),
    };

    let session = state.session_mut(&session_id).unwrap();
    session.active_agents.insert(
        "task-child".into(),
        crate::api::AgentInfo {
            agent_instance_id: "task-child".into(),
            agent_id: "hello-agent".into(),
            parent_agent_instance_id: Some("task-main".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running {
                execution_id: "task-child".into(),
            },
            unread_report_count: 0,
            name: "hello-agent".into(),
            role: "assistant".into(),
            status: crate::api::AgentStatus::Running,
        },
    );
    session.active_agents.insert(
        "task-main".into(),
        crate::api::AgentInfo {
            agent_instance_id: "task-main".into(),
            agent_id: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running {
                execution_id: "task-main".into(),
            },
            unread_report_count: 0,
            name: "main".into(),
            role: "assistant".into(),
            status: crate::api::AgentStatus::Running,
        },
    );

    let agents = state.get_agent_list(&session_id);
    assert_eq!(agents[0].agent_instance_id, "task-main");
    assert_eq!(agents[1].agent_instance_id, "task-child");
}
