use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::api::{AgentApiError, AgentRuntime};
use orchd::testing::CollectingPersistSink;
use orchd::testing::Supervisor;
use orchd_api::{
    MessageCommit, PersistAck, PersistError, PersistSink, TaskEventCommit, WorkEventCommit,
};

use super::support::{sample_create_request, sample_submit_input, test_agent_spec, test_config};
use crate::faux_provider::FauxProvider;

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
