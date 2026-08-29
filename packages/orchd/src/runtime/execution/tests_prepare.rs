use super::*;

#[tokio::test]
async fn prepare_failure_leaves_no_second_reservation_and_can_retry() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();
    let first = runtime
        .prepare_execution(
            request_with("first", "message-first"),
            HashMap::new(),
            tracing::Span::none(),
        )
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .prepare_execution(
                request_with("second", "message-second"),
                HashMap::new(),
                tracing::Span::none()
            )
            .await,
        Err(AgentApiError::ExecutionAlreadyActive)
    ));
    let scope = runtime.scope("session").await.unwrap();
    assert!(scope.get_execution("second").await.is_none());
    first.rollback().await;
    let retry = runtime
        .prepare_execution(
            request_with("second", "message-second"),
            HashMap::new(),
            tracing::Span::none(),
        )
        .await
        .unwrap();
    retry.rollback().await;
}
