use hostd::turn_runner::{MockTurnRunner, TurnRunInput, TurnRunner};

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockTurnRunner;
    let output = runner
        .run_turn(
            TurnRunInput {
                session_id: "session-test".into(),
                turn_id: "turn-test".into(),
                prompt: "hello".into(),
                system_prompt: "system prompt".into(),
                cwd: "".into(),
                active_tool_names: None,
            },
            None,
        )
        .await
        .unwrap();

    assert!(output.events.is_empty());
    assert_eq!(output.total_tasks, 1);
}

#[tokio::test]
async fn turn_runner_can_emit_streaming_events_to_channel() {
    let runner = MockTurnRunner;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let output = runner
        .run_turn(
            TurnRunInput {
                session_id: "session-test".into(),
                turn_id: "turn-test".into(),
                prompt: "hello".into(),
                system_prompt: "system prompt".into(),
                cwd: "".into(),
                active_tool_names: None,
            },
            Some(tx),
        )
        .await
        .unwrap();

    assert!(output.events.is_empty());
    assert_eq!(output.total_tasks, 1);
    drop(rx);
}
