use orchd::api::{AgentApiError, AgentRuntime};

use piko_protocol::MessageContent;

use super::support::{sample_create_request, sample_submit_input, setup_runtime};

#[tokio::test]
async fn submit_input_conflicts_on_reused_request_id_with_different_payload() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    let input = sample_submit_input(&handle.task_id);
    runtime.submit_input(input).await.unwrap();

    let mut conflict = sample_submit_input(&handle.task_id);
    conflict.content = MessageContent::String("different".into());
    let error = runtime.submit_input(conflict).await.unwrap_err();
    assert_eq!(error, AgentApiError::IdempotencyConflict);
}
