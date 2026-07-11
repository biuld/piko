//! Agent API behavior tests (create/submit/control/idempotency).

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::api::{AgentApiError, AgentRuntime};
use orchd::host::Supervisor;
use orchd::integration::PersistSink;
use orchd::integration::{
    MessageCommit, PersistAck, PersistError, TaskEventCommit, WorkEventCommit,
};
use orchd::testing::CollectingPersistSink;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputDelivery, InputDisposition, InputSource, SessionCursor,
    SubmitTaskInput, SubscribeRequest, TaskControlRequest, TaskMode, TaskStatus,
};
use piko_protocol::agents::{AgentSpec, HostTaskContext};
use piko_protocol::config::OrchdConfig;

use futures_util::StreamExt;

mod faux_provider;
use faux_provider::FauxProvider;

struct RejectMessageSink {
    inner: CollectingPersistSink,
}

#[async_trait::async_trait]
impl PersistSink for RejectMessageSink {
    async fn commit_message(&self, _event: MessageCommit) -> Result<PersistAck, PersistError> {
        Err(PersistError::Failed("injected message failure".into()))
    }

    async fn commit_task_event(&self, event: TaskEventCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_task_event(event).await
    }

    async fn commit_work_event(&self, event: WorkEventCommit) -> Result<PersistAck, PersistError> {
        self.inner.commit_work_event(event).await
    }
}

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
    core.set_persist_sink(Arc::new(CollectingPersistSink::new()) as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    (core, runtime)
}

#[tokio::test]
async fn create_task_fails_closed_without_persistence() {
    let faux = Arc::new(FauxProvider::new());
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
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
async fn message_persistence_failure_prevents_model_side_effect() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("must not be called").await;
    let core = Supervisor::from_config(
        faux.clone() as Arc<dyn llmd::gateway::LlmGateway>,
        test_config(),
    )
    .await;
    core.set_persist_sink(Arc::new(RejectMessageSink {
        inner: CollectingPersistSink::new(),
    }) as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(core);
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    assert!(matches!(
        runtime
            .submit_input(sample_submit_input(&handle.task_id))
            .await,
        Err(AgentApiError::PersistenceFailed(_))
    ));
    tokio::task::yield_now().await;
    assert_eq!(faux.call_count().await, 0);
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
        host_context: HostTaskContext::new("session-idem"),
        resume: None,
    }
}

fn sample_submit_input(task_id: &str) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: "req-input-1".into(),
        session_id: "session-idem".into(),
        task_id: task_id.to_string(),
        message_id: "msg-input-1".into(),
        work_id: "work-idem-1".into(),
        source_turn_id: None,
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

#[tokio::test]
async fn duplicate_submit_only_commits_user_message_once_with_persist_sink() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("idempotent response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
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
    let work_events = sink.work_events();
    assert!(work_events.iter().any(|event| {
        event.snapshot.work_id == "work-idem-1"
            && event.snapshot.status == piko_protocol::agent_runtime::WorkStatus::Running
    }));
    assert!(work_events.iter().any(|event| {
        event.snapshot.work_id == "work-idem-1"
            && event.snapshot.status == piko_protocol::agent_runtime::WorkStatus::Succeeded
    }));
}

#[tokio::test]
async fn session_hub_receives_task_changed_on_create() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    let subscription = runtime
        .subscribe_session(SubscribeRequest {
            session_id: "session-idem".into(),
            task_id: None,
            after: None,
        })
        .await
        .unwrap();
    let mut output = subscription.output;

    let _ = runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut saw_task_changed = false;
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(Ok(envelope))) =
            tokio::time::timeout(std::time::Duration::from_millis(200), output.next()).await
            && let piko_protocol::agent_runtime::SessionOutput::Event(event) = envelope.output
            && matches!(
                event.event,
                piko_protocol::agent_runtime::SessionEvent::TaskChanged { .. }
            )
        {
            saw_task_changed = true;
            break;
        }
    }
    assert!(saw_task_changed, "expected TaskChanged on session hub");
}

#[tokio::test]
async fn invalid_subscription_cursor_is_reported_on_stream() {
    let (_core, runtime) = setup_runtime().await;
    let subscription = runtime
        .subscribe_session(SubscribeRequest {
            session_id: "session-idem".into(),
            task_id: None,
            after: Some(SessionCursor {
                epoch: "stale-epoch".into(),
                seq: 0,
            }),
        })
        .await
        .unwrap();
    let mut output = subscription.output;
    assert!(matches!(
        output.next().await,
        Some(Err(orchd::api::SessionStreamError::SnapshotRequired {
            reason: orchd::api::SnapshotRequiredReason::EpochChanged,
        }))
    ));
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn session_snapshot_excludes_tasks_from_other_sessions() {
    let (_core, runtime) = setup_runtime().await;
    runtime.create_task(sample_create_request()).await.unwrap();
    let mut other = sample_create_request();
    other.request_id = "req-create-other".into();
    other.session_id = "session-other".into();
    other.task_id = Some("task-other".into());
    other.host_context = HostTaskContext::new("session-other");
    runtime.create_task(other).await.unwrap();

    let snapshot = runtime
        .session_snapshot("session-idem".into())
        .await
        .unwrap();
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].task_id, "task_idem_root");
}

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

#[tokio::test]
async fn resumed_task_continues_persisted_sequence_without_recreating_task() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("resumed response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(core);
    let mut request = sample_create_request();
    request.resume = Some(piko_protocol::agent_runtime::TaskResumeState {
        transcript: vec![piko_protocol::Message::User {
            content: MessageContent::String("old input".into()),
            timestamp: Some(1),
        }],
        head_message_id: Some("msg-old".into()),
        last_task_seq: 7,
        committed_message_ids: vec!["msg-old".into()],
    });
    let handle = runtime.create_task(request).await.unwrap();
    runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await
        .unwrap();

    let commit = sink
        .messages()
        .into_iter()
        .find(|commit| commit.message_id == "msg-input-1")
        .unwrap();
    assert_eq!(commit.task_seq, 8);
    assert_eq!(commit.parent_message_id.as_deref(), Some("msg-old"));
    assert!(
        sink.task_events()
            .into_iter()
            .all(|commit| { !matches!(commit.event, piko_protocol::TaskEvent::Created { .. }) })
    );
}
