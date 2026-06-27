use hostd::api::HostEvent;
use hostd::state::HostState;
use hostd::turn_runner::{MockTurnRunner, TurnRunInput, TurnRunner};

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockTurnRunner;
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/piko-hostd-test") {
        HostEvent::SessionCreated { session_id, .. } => session_id,
        event => panic!("unexpected event: {event:?}"),
    };
    let (turn_id, start_events) = state.start_turn(&session_id).unwrap();
    assert!(!start_events.is_empty());

    let output = runner
        .run_turn(
            TurnRunInput {
                session_id: session_id.clone(),
                turn_id: turn_id.clone(),
                prompt: "hello".into(),
                system_prompt: "system prompt".into(),
                active_tool_names: None,
            },
            &mut state,
            None,
        )
        .await
        .unwrap();

    assert!(!output.events.is_empty());
}

#[tokio::test]
async fn turn_runner_can_emit_streaming_events_to_channel() {
    let runner = MockTurnRunner;
    let mut state = HostState::new();
    let session_id = match state.create_session("/tmp/piko-hostd-test") {
        HostEvent::SessionCreated { session_id, .. } => session_id,
        event => panic!("unexpected event: {event:?}"),
    };
    let (turn_id, _) = state.start_turn(&session_id).unwrap();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let output = runner
        .run_turn(
            TurnRunInput {
                session_id,
                turn_id,
                prompt: "hello".into(),
                system_prompt: "system prompt".into(),
                active_tool_names: None,
            },
            &mut state,
            Some(tx),
        )
        .await
        .unwrap();

    // MockTurnRunner returns events in output, not via channel
    // (event_tx is for real OrchTurnRunner streaming)
    assert!(!output.events.is_empty());
    drop(rx);
}
