use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use piko_llmd::gateway::{GatewayEvent, GatewayRequest};
use piko_protocol::execution::{CommitAck, CommitError};

use super::*;

struct NoopGateway;

#[async_trait]
impl LlmGateway for NoopGateway {
    async fn chat_stream(
        &self,
        _req: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Ok(Box::pin(tokio_stream::empty()))
    }

    async fn llm_call(
        &self,
        _model: piko_protocol::Model,
        _system_prompt: Option<String>,
        _messages: Vec<piko_protocol::Message>,
        _settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        Ok(String::new())
    }

    fn capabilities(&self) -> piko_protocol::model::ModelCapabilities {
        piko_protocol::model::ModelCapabilities::default()
    }
}

struct NoopCommit;

#[async_trait]
impl piko_orchd_api::ExecutionCommitPort for NoopCommit {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        Ok(CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: 1,
        })
    }
}

fn request() -> StartExecutionRequest {
    StartExecutionRequest {
        request_id: "request".into(),
        session_id: "session".into(),
        source_turn_id: None,
        execution_id: "execution".into(),
        agent_instance_id: "agent".into(),
        agent_spec: AgentSpec {
            id: "main".into(),
            name: "main".into(),
            role: "test".into(),
            description: None,
            base_system_prompt: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
        run_prompt: piko_protocol::AgentRunPrompt {
            system_prompt: String::new(),
            assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            source_digest: "digest".into(),
        },
        input_message_id: "message".into(),
        input: piko_protocol::MessageContent::String("hello".into()),
        context: piko_protocol::ConversationContext::empty(),
        config: piko_protocol::ExecutionConfig {
            agent_id: "main".into(),
            ..Default::default()
        },
    }
}

fn request_with(execution_id: &str, message_id: &str) -> StartExecutionRequest {
    StartExecutionRequest {
        execution_id: execution_id.into(),
        input_message_id: message_id.into(),
        ..request()
    }
}

#[tokio::test]
async fn dropping_prepared_execution_releases_its_reservation() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();
    let prepared = runtime
        .prepare_execution(request(), Vec::new(), HashMap::new())
        .await
        .unwrap();
    let scope = runtime.scope("session").await.unwrap();
    assert!(scope.get_execution("execution").await.is_some());
    drop(prepared);
    for _ in 0..100 {
        if scope.get_execution("execution").await.is_none() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("dropping PreparedExecution leaked its reservation");
}

#[tokio::test]
async fn aborting_task_that_owns_prepared_execution_releases_reservation() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();
    let prepared = runtime
        .prepare_execution(request(), Vec::new(), HashMap::new())
        .await
        .unwrap();
    let scope = runtime.scope("session").await.unwrap();
    let owner = tokio::spawn(async move {
        let _prepared = prepared;
        std::future::pending::<()>().await;
    });
    owner.abort();
    let _ = owner.await;
    for _ in 0..100 {
        if scope.get_execution("execution").await.is_none() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("aborting PreparedExecution owner leaked its reservation");
}

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
            Vec::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .prepare_execution(
                request_with("second", "message-second"),
                Vec::new(),
                HashMap::new(),
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
            Vec::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
    retry.rollback().await;
}
