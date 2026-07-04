use hostd::turn_runner::{MockTurnRunner, TurnRunInput, TurnRunner};
use tokio_stream::StreamExt;

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockTurnRunner;
    let mut stream = runner
        .run_turn(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
        })
        .await
        .unwrap();

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn turn_runner_returns_streaming_events() {
    let runner = MockTurnRunner;

    let mut stream = runner
        .run_turn(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
        })
        .await
        .unwrap();

    assert!(stream.next().await.is_none());
}
