use orchd::AgentRuntimeService;
use orchd::api::{AgentApiError, AgentRuntime};
use orchd::testing::Supervisor;

use super::support::{sample_create_request, setup_runtime, test_agent_spec, test_config};

#[tokio::test]
async fn create_task_fails_closed_without_persistence() {
    let faux = std::sync::Arc::new(crate::faux_provider::FauxProvider::new());
    let core =
        Supervisor::from_config(faux as std::sync::Arc<dyn llmd::gateway::LlmGateway>, test_config())
            .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(core);

    assert_eq!(
        runtime
            .create_task(sample_create_request())
            .await
            .unwrap_err(),
        AgentApiError::PersistenceUnavailable
    );
}

#[tokio::test]
async fn create_task_retries_return_same_handle() {
    let (_core, runtime) = setup_runtime().await;
    let request = sample_create_request();

    let first = runtime.create_task(request.clone()).await.unwrap();
    let second = runtime.create_task(request).await.unwrap();

    assert_eq!(first, second);
}

#[tokio::test]
async fn create_task_conflicts_on_reused_request_id_with_different_payload() {
    let (_core, runtime) = setup_runtime().await;
    let first = sample_create_request();
    runtime.create_task(first).await.unwrap();

    let mut conflict = sample_create_request();
    conflict.agent_id = "other".into();
    let error = runtime.create_task(conflict).await.unwrap_err();
    assert_eq!(error, AgentApiError::IdempotencyConflict);
}
