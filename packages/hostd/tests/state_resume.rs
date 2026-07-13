use hostd::HostState;
use hostd::api::ServerMessage as Event;

#[test]
fn start_turn_rejects_second_active_turn() {
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/project") {
        hostd::api::CommandResult::SessionCreated { session_id, .. } => session_id,
        _ => panic!("expected session_created"),
    };

    let (turn_id, _) = state.start_turn(&session_id).unwrap();
    let err = state.start_turn(&session_id).unwrap_err();
    assert!(matches!(
        err,
        hostd::api::ProtocolError::ActiveTurnExists(_)
    ));

    state.complete_turn(&session_id, &turn_id).unwrap();
    assert!(state.start_turn(&session_id).is_ok());
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

    let (turn_id, events) = state.start_turn(&session_id).unwrap();
    assert!(events.is_empty());

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

    let (turn_id, _) = state.start_turn(&session_id).unwrap();
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

    let (turn_id, _) = state.start_turn(&session_id).unwrap();
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

    let (turn_id, _) = state.start_turn(&session_id).unwrap();
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
    assert!(snapshot.active_turn.is_none());

    // Idempotent when no active turn remains.
    assert!(
        state
            .finalize_interrupted_turns(&session_id)
            .unwrap()
            .is_empty()
    );
    assert!(state.start_turn(&session_id).is_ok());
}
