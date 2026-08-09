use super::*;

#[tokio::test]
async fn responses_uses_native_stream_and_non_streaming_contracts() {
    use piko_llmd::gateway::{ModelEvent, SemanticItem};
    let streaming = Stub::start(Script {
        steps: vec![Step::Status(503), Step::ResponsesStreamSuccess],
    })
    .await;
    let executor = executor_for_protocol(
        streaming.addr,
        None,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default(),
        },
    );
    let events = executor
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    assert!(events.iter().any(|event| matches!(event,
        ModelEvent::TextDelta { delta, identity }
            if delta == "native" && identity.response_id.as_deref() == Some("resp_1")
                && identity.item_id.as_deref() == Some("msg_1")
    )));
    assert_eq!(streaming.request_count(), 2);

    let once = Stub::start(Script {
        steps: vec![Step::ResponsesNonStreaming],
    })
    .await;
    let result = executor_for_protocol(
        once.addr,
        None,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default(),
        },
    )
    .execute_once(request(), tokio_util::sync::CancellationToken::new())
    .await
    .unwrap();
    assert!(
        matches!(&result.items[0], SemanticItem::Text { text, identity }
        if text == "native" && identity.item_id.as_deref() == Some("msg_1"))
    );
    let continuation = result.output_metadata.continuation.unwrap();
    assert_eq!(
        continuation.adapter,
        piko_llmd::modeling::ProtocolKind::Responses.adapter_id()
    );
    assert_eq!(continuation.state["response_id"], "resp_1");
}

#[tokio::test]
async fn captures_actual_model_input_before_provider_dispatch() {
    let stub = Stub::start(Script {
        steps: vec![Step::StreamSuccess],
    })
    .await;
    let capture = Arc::new(PromptCapture::default());
    let stream = executor(stub.addr, Some(true))
        .with_telemetry(capture.clone())
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await
        .unwrap();
    let _ = stream.collect::<Vec<_>>().await;

    let inputs = capture.0.lock().unwrap();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].session_id, "session-1");
    assert_eq!(inputs[0].agent_instance_id, "agent-1");
    assert_eq!(inputs[0].provider, "openai");
    assert_eq!(inputs[0].request["messages"][0]["content"], "hi");
    assert_eq!(inputs[0].options["protocol"], "chat_completions");
}

#[tokio::test]
async fn responses_falls_back_only_to_responses_non_streaming() {
    let stub = Stub::start(Script {
        steps: vec![
            Step::Status(503),
            Step::Status(503),
            Step::Status(503),
            Step::ResponsesNonStreaming,
        ],
    })
    .await;
    let events = executor_for_protocol(
        stub.addr,
        None,
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default(),
        },
    )
    .execute(request(), tokio_util::sync::CancellationToken::new())
    .await
    .unwrap()
    .collect::<Vec<_>>()
    .await;
    assert!(events.iter().any(|event| matches!(event, piko_llmd::gateway::ModelEvent::TextDelta { delta, .. } if delta == "native")));
    assert_eq!(stub.streaming_count(), 3);
    assert_eq!(stub.non_streaming_count(), 1);
}

#[tokio::test]
async fn cancellation_is_typed_before_dispatch_for_both_protocols() {
    use piko_llmd::gateway::ErrorClass;
    for protocol in [
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default(),
        },
        piko_llmd::modeling::ProtocolProfile::ChatCompletions,
    ] {
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let error = match executor_for_protocol("127.0.0.1:9".parse().unwrap(), None, protocol)
            .execute(request(), cancel)
            .await
        {
            Ok(_) => panic!("cancelled execution unexpectedly opened a stream"),
            Err(error) => error,
        };
        assert_eq!(error.class, ErrorClass::Cancelled);
    }
}

#[tokio::test]
async fn cancellation_interrupts_backoff_with_typed_error() {
    use piko_llmd::gateway::ErrorClass;
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503)],
    })
    .await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let task_cancel = cancel.clone();
    let executor = executor(stub.addr, None).with_retry(RetryConfig {
        enabled: true,
        max_retries: 2,
        base_delay_ms: 5_000,
        max_delay_ms: 5_000,
        budget_ms: 10_000,
    });
    let task = tokio::spawn(async move { executor.execute(request(), task_cancel).await });
    while stub.request_count() == 0 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();
    let error = match task.await.unwrap() {
        Ok(_) => panic!("cancelled backoff unexpectedly opened a stream"),
        Err(error) => error,
    };
    assert_eq!(error.class, ErrorClass::Cancelled);
    assert_eq!(stub.request_count(), 1);
}

#[tokio::test]
async fn usage_and_cost_middleware_process_both_protocols() {
    use piko_llmd::middleware::cost_tracker::CostTrackerMiddleware;
    use piko_llmd::middleware::token_usage::TokenUsageMiddleware;
    use std::sync::atomic::Ordering;

    for (protocol, step, expected_input) in [
        (
            piko_llmd::modeling::ProtocolProfile::Responses {
                continuation: Default::default(),
            },
            Step::ResponsesStreamSuccess,
            4,
        ),
        (
            piko_llmd::modeling::ProtocolProfile::ChatCompletions,
            Step::StreamSuccess,
            3,
        ),
    ] {
        let stub = Stub::start(Script { steps: vec![step] }).await;
        let usage = Arc::new(TokenUsageMiddleware::new());
        let mut req = request();
        req.model = "gpt-4o".into();
        let events = executor_for_protocol(stub.addr, None, protocol)
            .add_middleware(Arc::new(CostTrackerMiddleware::new()))
            .add_middleware(usage.clone())
            .execute(req, tokio_util::sync::CancellationToken::new())
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(usage.total_input.load(Ordering::SeqCst), expected_input);
        assert!(events.iter().any(|event| matches!(event,
            piko_llmd::gateway::ModelEvent::Usage(value) if value.cost.total > 0.0
        )));
    }
}

#[tokio::test]
async fn malformed_json_fails_deterministically_for_both_protocols() {
    use piko_llmd::gateway::ErrorClass;
    for protocol in [
        piko_llmd::modeling::ProtocolProfile::Responses {
            continuation: Default::default(),
        },
        piko_llmd::modeling::ProtocolProfile::ChatCompletions,
    ] {
        let stub = Stub::start(Script {
            steps: vec![Step::MalformedJson],
        })
        .await;
        let error = executor_for_protocol(stub.addr, None, protocol)
            .execute_once(request(), tokio_util::sync::CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.class, ErrorClass::Protocol);
        assert!(error.message.starts_with("malformed JSON response:"));
    }
}

#[tokio::test]
async fn fallback_disabled_fails_with_streaming_error() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503)],
    })
    .await;
    let result = executor(stub.addr, Some(false))
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await;
    assert!(result.is_err(), "fallback disabled must fail closed");
    assert_eq!(stub.request_count(), 3, "retries still occur; no fallback");
    assert_eq!(stub.non_streaming_count(), 0);
}

#[tokio::test]
async fn retry_disabled_also_disables_fallback() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(503), Step::NonStreaming],
    })
    .await;
    let result = executor(stub.addr, None)
        .with_retry(RetryConfig {
            enabled: false,
            ..retry_config()
        })
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await;
    assert!(result.is_err());
    assert_eq!(stub.request_count(), 1);
    assert_eq!(stub.non_streaming_count(), 0);
}

#[tokio::test]
async fn transport_break_before_output_uses_same_protocol_fallback() {
    let stub = Stub::start(Script {
        steps: vec![Step::StreamBreakBeforeOutput, Step::NonStreaming],
    })
    .await;
    let events = executor(stub.addr, None)
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    assert!(events.iter().any(|event| matches!(
        event,
        piko_llmd::gateway::ModelEvent::TextDelta { delta, .. }
            if delta == "fallback text"
    )));
    assert_eq!(stub.streaming_count(), 1);
    assert_eq!(stub.non_streaming_count(), 1);
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
            .any(|event| matches!(event, ModelEvent::Error(_)))
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ModelEvent::Completed(_)))
    );
    assert_eq!(stub.request_count(), 1);
}

#[tokio::test]
async fn non_retryable_open_fails_immediately() {
    let stub = Stub::start(Script {
        steps: vec![Step::Status(401)],
    })
    .await;
    let result = executor(stub.addr, None)
        .execute(request(), tokio_util::sync::CancellationToken::new())
        .await;
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
