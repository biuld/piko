use hostd::HostState;
use hostd::api::ServerMessage as Event;

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
    assert!(!events.is_empty());
    assert!(matches!(
        events[0],
        Event::Display(piko_protocol::Event::TurnLifecycle(
            hostd::api::TurnEvent::Started { .. }
        ))
    ));

    let complete = state.complete_turn(&session_id, &turn_id).unwrap();
    assert!(matches!(
        complete,
        Event::Display(piko_protocol::Event::TurnLifecycle(
            hostd::api::TurnEvent::Completed { .. }
        ))
    ));
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
        Event::Display(piko_protocol::Event::TurnLifecycle(
            hostd::api::TurnEvent::Failed { .. }
        ))
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
        Event::Display(piko_protocol::Event::TurnLifecycle(
            hostd::api::TurnEvent::Cancelled { .. }
        ))
    ));
}
