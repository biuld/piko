//! Task cancellation and termination.

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::api::{AgentApiError, AgentRuntime};
use piko_protocol::agent_runtime::TaskControlRequest;
use piko_protocol::agents::HostTaskContext;

use crate::faux_provider::FauxProvider;
use crate::runtime::{
    test_agent_spec, test_config, test_supervisor, wait_for_task_report, wait_for_task_status,
};

#[tokio::test]
async fn test_cancel_task() {
    let config = test_config();
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let spec = test_agent_spec("cancellable");
    core.register_agent(spec).await;

    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    assert_eq!(
        runtime
            .control_task(TaskControlRequest::Terminate {
                request_id: "req-cancel-missing".into(),
                task_id: "nonexistent-task".into(),
            })
            .await
            .unwrap_err(),
        AgentApiError::TaskNotFound
    );
}

#[tokio::test]
async fn test_cancelled_task_runtime_is_unregistered() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("ready").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("cancellable")).await;

    let task_id = core
        .spawn_detached(
            "cancellable",
            "wait",
            None,
            None,
            HostTaskContext::new("session_cancel"),
        )
        .await;
    wait_for_task_report(&core, &task_id).await;

    AgentRuntimeService::new(Arc::clone(&core))
        .control_task(TaskControlRequest::Terminate {
            request_id: "req-cancel-task".into(),
            task_id: task_id.clone(),
        })
        .await
        .unwrap();
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Cancelled,
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while core.steer_task(&task_id, "should fail").await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("cancelled runtime handle should be removed");
}
