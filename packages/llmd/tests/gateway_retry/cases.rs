use super::*;

#[tokio::test]
async fn fallback_disabled_fails_with_streaming_error() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503)],
    })
    .await;
    let result = executor(stub.addr, Some(false))
        .chat_stream(request(), None)
        .await;
    assert!(result.is_err(), "fallback disabled must fail closed");
    assert_eq!(stub.request_count(), 3, "retries still occur; no fallback");
    assert_eq!(stub.non_streaming_count(), 0);
}

#[tokio::test]
async fn mid_stream_break_surfaces_error_without_restart() {
    let stub = Stub::start(Script {
        steps: vec![Step::StreamPartialThenClose, Step::StreamSuccess],
    })
    .await;
    let events = collect(&executor(stub.addr, None), request()).await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, GatewayEvent::Error(_)))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, GatewayEvent::Done(_)))
    );
    assert_eq!(stub.request_count(), 1);
}

#[tokio::test]
async fn non_retryable_open_fails_immediately() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(401)],
    })
    .await;
    let result = executor(stub.addr, None).chat_stream(request(), None).await;
    assert!(result.is_err());
    assert_eq!(stub.request_count(), 1, "no retries for 401");
}

#[tokio::test]
async fn llm_call_retries_transient_errors() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503), Step::NonStreaming],
    })
    .await;
    let model = piko_protocol::messages::Model {
        id: "gpt-test".into(),
        name: "gpt-test".into(),
        provider: "openai".into(),
        base_url: None,
    };
    let out = executor(stub.addr, None)
        .llm_call(
            model,
            None,
            vec![],
            piko_protocol::model::ModelRunSettings::default(),
        )
        .await
        .expect("llm_call should retry and succeed");
    assert_eq!(out, "fallback text");
    assert_eq!(stub.request_count(), 2, "one failure then one success");
    assert_eq!(stub.non_streaming_count(), 2);
}
