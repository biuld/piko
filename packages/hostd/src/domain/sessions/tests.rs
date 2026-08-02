use super::*;
use crate::api::{RealtimeMessageEvent, ServerMessage};
use piko_protocol::agent_runtime::RealtimeDelta;

fn realtime(agent_instance_id: &str, agent_id: &str, message_id: &str, seq: u64) -> ServerMessage {
    ServerMessage::RealtimeMessage(RealtimeMessageEvent {
        session_id: "session".into(),
        agent_instance_id: agent_instance_id.into(),
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
fn record_turn_model_reports_previous_model_and_tracks_continuity() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp") {
        crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session created"),
    };
    let model_a = super::types::SessionModelRef::new("openai", "model-a");
    let model_b = super::types::SessionModelRef::new("openai", "model-b");
    let model_b2 = super::types::SessionModelRef::new("anthropic", "model-b");

    // First turn: no previous model; current model is recorded.
    let previous = state
        .record_turn_model(&session_id, Some(&model_a))
        .unwrap();
    assert_eq!(previous, None);

    // Second turn on the same model: no switch.
    let previous = state
        .record_turn_model(&session_id, Some(&model_a))
        .unwrap();
    assert_eq!(previous.as_ref(), Some(&model_a));

    // Model change is observable by the next caller.
    let previous = state
        .record_turn_model(&session_id, Some(&model_b))
        .unwrap();
    assert_eq!(previous.as_ref(), Some(&model_a));

    // A provider change is also a model change even with the same model id.
    let previous = state
        .record_turn_model(&session_id, Some(&model_b2))
        .unwrap();
    assert_eq!(previous.as_ref(), Some(&model_b));

    // An unconfigured model does not erase recorded history.
    let previous = state.record_turn_model(&session_id, None).unwrap();
    assert_eq!(previous.as_ref(), Some(&model_b2));
}

#[test]
fn record_world_state_returns_previous_facts_and_tracks_baseline() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp") {
        crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session created"),
    };
    let facts = |operation_id: &str, run_kind: crate::domain::prompts::RunKind| {
        crate::domain::prompts::WorldStateFacts {
            session_id: Some(session_id.clone()),
            agent_instance_id: Some("agent_root".into()),
            operation_id: Some(operation_id.into()),
            run_kind,
            model: Some("model-a".into()),
        }
    };

    // First turn: no baseline → full injection is triggered upstream.
    let first = facts("turn_1", crate::domain::prompts::RunKind::Initial);
    assert_eq!(state.record_world_state(&session_id, &first).unwrap(), None);

    // Second turn: previous facts become the diff baseline.
    let second = facts("turn_2", crate::domain::prompts::RunKind::Continuation);
    let previous = state.record_world_state(&session_id, &second).unwrap();
    assert_eq!(previous, Some(first));
    assert_eq!(
        state
            .session(&session_id)
            .unwrap()
            .world_state_baseline
            .as_ref(),
        Some(&second)
    );
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
            session_id: session_id.clone(),
            agent_instance_id: "task-child".into(),
            agent_id: "hello-agent".into(),
            parent_agent_instance_id: Some("task-main".into()),
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running,
            unread_report_count: 0,
            name: "hello-agent".into(),
            role: "assistant".into(),
            status: crate::api::AgentStatus::Running,
        },
    );
    session.active_agents.insert(
        "task-main".into(),
        crate::api::AgentInfo {
            session_id: session_id.clone(),
            agent_instance_id: "task-main".into(),
            agent_id: "main".into(),
            parent_agent_instance_id: None,
            lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
            activity: piko_protocol::AgentActivity::Running,
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

#[test]
fn upsert_live_agent_makes_subscribe_snapshot_available() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp") {
        crate::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session created"),
    };

    state
        .upsert_live_agent(
            &session_id,
            crate::api::AgentInfo {
                session_id: session_id.clone(),
                agent_instance_id: "agent_spawn_1".into(),
                agent_id: "coder".into(),
                parent_agent_instance_id: Some("root".into()),
                lifecycle: piko_protocol::AgentInstanceLifecycle::Open,
                activity: piko_protocol::AgentActivity::Idle,
                unread_report_count: 0,
                name: "Coder".into(),
                role: "assistant".into(),
                status: crate::api::AgentStatus::Idle,
            },
        )
        .unwrap();

    let snapshot = state
        .agent_view_snapshot(&session_id, "agent_spawn_1")
        .unwrap();
    assert_eq!(snapshot.agent_id, "coder");
    assert_eq!(snapshot.agent_instance_id, "agent_spawn_1");
}
