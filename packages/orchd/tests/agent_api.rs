//! Agent API behavior tests (create/submit/control/idempotency).

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::Supervisor;
use orchd::adapters::persist::CollectingPersistSink;
use orchd::api::{AgentApiError, AgentRuntime};
use orchd::integration::PersistSink;
use orchd::protocol::agents::{AgentSpec, HostTaskContext};
use orchd::protocol::config::OrchdConfig;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputDelivery, InputDisposition, InputSource, SubmitTaskInput, TaskMode,
};

mod faux_provider;
use faux_provider::FauxProvider;

fn test_config() -> OrchdConfig {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();
    config
}

fn test_agent_spec(id: &str) -> AgentSpec {
    AgentSpec {
        id: id.to_string(),
        name: id.to_string(),
        role: "test".to_string(),
        description: None,
        system_prompt: "You are a test agent.".to_string(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    }
}

async fn setup_runtime() -> (Arc<Supervisor>, AgentRuntimeService) {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("idempotent response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    (core, runtime)
}

fn sample_create_request() -> CreateTaskRequest {
    CreateTaskRequest {
        request_id: "req-create-1".into(),
        session_id: "session-idem".into(),
        task_id: Some("task_idem_root".into()),
        agent_id: "idem".into(),
        parent_task_id: None,
        source: InputSource::User,
        mode: TaskMode::Attached,
        host_context: HostTaskContext {
            session_id: "session-idem".into(),
            turn_id: "work-idem-1".into(),
        },
        initial_history: None,
    }
}

fn sample_submit_input(task_id: &str) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: "req-input-1".into(),
        session_id: "session-idem".into(),
        task_id: task_id.to_string(),
        message_id: "msg-input-1".into(),
        work_id: "work-idem-1".into(),
        source: InputSource::User,
        content: MessageContent::String("hello".into()),
        delivery: InputDelivery::AfterCurrentStep,
        submitted_at: 1,
    }
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

#[tokio::test]
async fn submit_input_retries_return_duplicate_receipt() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    let input = sample_submit_input(&handle.task_id);
    let first = runtime.submit_input(input.clone()).await.unwrap();
    assert_eq!(first.disposition, InputDisposition::Accepted);

    let second = runtime.submit_input(input).await.unwrap();
    assert_eq!(second.disposition, InputDisposition::Duplicate);
    assert_eq!(second.message_id, first.message_id);
}

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

#[tokio::test]
async fn duplicate_submit_only_commits_user_message_once_with_persist_sink() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("idempotent response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(Some(sink.clone() as Arc<dyn PersistSink>))
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));

    let handle = runtime.create_task(sample_create_request()).await.unwrap();
    let input = sample_submit_input(&handle.task_id);

    runtime.submit_input(input.clone()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    runtime.submit_input(input).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let commits: Vec<_> = sink
        .messages()
        .into_iter()
        .filter(|commit| commit.message_id == "msg-input-1")
        .collect();
    assert_eq!(commits.len(), 1);
}
