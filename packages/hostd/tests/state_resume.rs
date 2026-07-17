use hostd::HostState;
use hostd::api::ServerMessage as Event;

#[test]
fn start_turn_queues_second_turn_for_same_agent() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "first")
        .unwrap();
    let first_status = state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let (queued_turn_id, second_status) = state
        .start_turn(&session_id, &agent_instance_id, "second")
        .unwrap();
    assert_eq!(first_status, hostd::api::TurnStatus::Running);
    assert_eq!(second_status, hostd::api::TurnStatus::Queued);

    state.complete_turn(&session_id, &turn_id).unwrap();
    state
        .mark_turn_running(&session_id, &queued_turn_id)
        .unwrap();
    assert_eq!(
        state.turn(&session_id, &queued_turn_id).unwrap().status,
        hostd::api::TurnStatus::Running
    );
}

#[test]
fn different_agents_can_own_running_turns_concurrently() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };
    let root_agent_instance_id = format!("agent_{session_id}_root");
    let (root_turn_id, _) = state
        .start_turn(&session_id, &root_agent_instance_id, "root input")
        .unwrap();
    let root_status = state
        .apply_turn_input_disposition(
            &session_id,
            &root_turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let (child_turn_id, _) = state
        .start_turn(&session_id, "agent-child", "child input")
        .unwrap();
    let child_status = state
        .apply_turn_input_disposition(
            &session_id,
            &child_turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();

    assert_eq!(root_status, hostd::api::TurnStatus::Running);
    assert_eq!(child_status, hostd::api::TurnStatus::Running);
    assert_eq!(state.snapshot(&session_id).unwrap().active_turns.len(), 2);
}

#[test]
fn create_session_emits_session_created() {
    let mut state = HostState::new();
    let event = state.create_session("/tmp/project");
    assert!(matches!(
        event,
        hostd::api::CommandResult::SessionCreated { .. }
    ));
}

#[test]
fn can_start_and_complete_turn() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "hello")
        .unwrap();
    let status = state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    assert_eq!(status, hostd::api::TurnStatus::Running);

    let complete = state.complete_turn(&session_id, &turn_id).unwrap();
    assert!(matches!(
        &complete,
        Event::TurnLifecycle(hostd::api::TurnEvent::Completed { .. })
    ));
    let replay = state.complete_turn(&session_id, &turn_id).unwrap();
    let Event::TurnLifecycle(hostd::api::TurnEvent::Completed {
        session_id: s1,
        turn_id: t1,
        ..
    }) = complete
    else {
        panic!("expected Completed turn event");
    };
    let Event::TurnLifecycle(hostd::api::TurnEvent::Completed {
        session_id: s2,
        turn_id: t2,
        ..
    }) = replay
    else {
        panic!("expected Completed turn event");
    };
    assert_eq!(s1, s2);
    assert_eq!(t1, t2);
}

#[test]
fn fail_turn_emits_turn_failed() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "fail")
        .unwrap();
    let fail = state
        .fail_turn(&session_id, &turn_id, "test error")
        .unwrap();
    assert!(matches!(
        fail,
        Event::TurnLifecycle(hostd::api::TurnEvent::Failed { .. })
    ));
}

#[test]
fn cancel_turn_emits_turn_cancelled() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "cancel")
        .unwrap();
    let cancel = state.cancel_turn(&session_id, &turn_id).unwrap();
    assert!(matches!(
        cancel,
        Event::TurnLifecycle(hostd::api::TurnEvent::Cancelled { .. })
    ));
}

#[test]
fn finalize_interrupted_turns_clears_active_turn_and_emits_failed() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let agent_instance_id = format!("agent_{session_id}_root");
    let (turn_id, _) = state
        .start_turn(&session_id, &agent_instance_id, "interrupt")
        .unwrap();
    state
        .apply_turn_input_disposition(
            &session_id,
            &turn_id,
            piko_protocol::InputDisposition::Accepted,
        )
        .unwrap();
    let events = state.finalize_interrupted_turns(&session_id).unwrap();
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::TurnLifecycle(hostd::api::TurnEvent::Failed {
            turn_id: failed_id,
            error,
            ..
        }) => {
            assert_eq!(failed_id, &turn_id);
            assert!(error.contains("interrupted"));
        }
        other => panic!("expected Failed turn event, got {other:?}"),
    }

    let snapshot = state.snapshot(&session_id).unwrap();
    assert!(snapshot.active_turns.is_empty());

    // Idempotent when no active turn remains.
    assert!(
        state
            .finalize_interrupted_turns(&session_id)
            .unwrap()
            .is_empty()
    );
    assert!(
        state
            .start_turn(&session_id, &agent_instance_id, "again")
            .is_ok()
    );
}
