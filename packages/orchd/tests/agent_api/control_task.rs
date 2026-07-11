use orchd::api::{AgentApiError, AgentRuntime};

use piko_protocol::agent_runtime::{TaskControlRequest, TaskStatus};

use super::support::{sample_create_request, sample_submit_input, setup_runtime};

#[tokio::test]
async fn control_task_is_idempotent_and_conflicts_on_payload_change() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if runtime
                .task_snapshot(handle.task_id.clone())
                .await
                .is_ok_and(|snapshot| snapshot.status == TaskStatus::Idle)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    let close = TaskControlRequest::Close {
        request_id: "req-control".into(),
        task_id: handle.task_id.clone(),
    };
    assert_eq!(
        runtime.control_task(close.clone()).await.unwrap().status,
        TaskStatus::Closed
    );
    assert_eq!(
        runtime.control_task(close).await.unwrap().status,
        TaskStatus::Closed
    );
    let conflict = TaskControlRequest::Reopen {
        request_id: "req-control".into(),
        task_id: handle.task_id,
    };
    assert_eq!(
        runtime.control_task(conflict).await.unwrap_err(),
        AgentApiError::IdempotencyConflict
    );
}
