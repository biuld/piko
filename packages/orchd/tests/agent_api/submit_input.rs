use orchd::api::{AgentApiError, AgentRuntime};

use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::InputDelivery;

use super::support::{sample_create_request, sample_submit_input, setup_runtime};

#[tokio::test]
async fn submit_input_retries_return_duplicate_receipt() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    let input = sample_submit_input(&handle.task_id);
    let first = runtime.submit_input(input.clone()).await.unwrap();
    assert_eq!(
        first.disposition,
        piko_protocol::agent_runtime::InputDisposition::Accepted
    );

    let second = runtime.submit_input(input).await.unwrap();
    assert_eq!(
        second.disposition,
        piko_protocol::agent_runtime::InputDisposition::Duplicate
    );
    assert_eq!(second.message_id, first.message_id);
}

#[tokio::test]
async fn immediate_input_is_rejected_while_work_is_active() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await
        .unwrap();

    let mut immediate = sample_submit_input(&handle.task_id);
    immediate.request_id = "req-input-immediate".into();
    immediate.message_id = "msg-input-immediate".into();
    immediate.work_id = "work-immediate".into();
    immediate.delivery = InputDelivery::Immediate;
    assert_eq!(
        runtime.submit_input(immediate).await.unwrap_err(),
        AgentApiError::InputRejected
    );
}
