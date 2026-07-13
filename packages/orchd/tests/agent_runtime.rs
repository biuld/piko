//! Vertical-slice tests for the long-lived AgentInstance control plane.

#[allow(dead_code)]
#[path = "common/faux_provider.rs"]
mod faux_provider;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use orchd::AgentRuntime;
use orchd::testing::CollectingExecutionCommitPort;
use orchd::tools::MultiAgentToolProvider;
use orchd_api::{
    AgentCommitPort, AgentRecoveryState, AgentRuntimeApi, SessionAgentConfig, SessionAgentPorts,
    SessionExecutionPorts, ToolExecutionContext, ToolProvider,
};
use piko_protocol::{
    AgentCommitAck, AgentDurableCommand, AgentInputDelivery, AgentInstanceIdentity,
    AgentInstanceLifecycle, AgentLifecycleRequest, AgentSpec, CommitError, CreateAgentRequest,
    MessageContent, SendAgentInputRequest,
};
use tokio::sync::Mutex;

use faux_provider::FauxProvider;

#[derive(Default)]
struct CollectingAgentCommitPort {
    revision: AtomicU64,
    commands: Mutex<Vec<AgentDurableCommand>>,
    fail_run_starts: AtomicU64,
    fail_run_terminals: AtomicU64,
}

struct FailingMessageCommitPort {
    attempt: AtomicU64,
    fail_at: u64,
}

#[async_trait]
impl orchd_api::ExecutionCommitPort for FailingMessageCommitPort {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<piko_protocol::CommitAck, CommitError> {
        let attempt = self.attempt.fetch_add(1, Ordering::SeqCst) + 1;
        if attempt == self.fail_at {
            return Err(CommitError::Unavailable);
        }
        Ok(piko_protocol::CommitAck {
            session_id: commit.session_id,
            execution_id: commit.execution_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: attempt,
        })
    }
}

impl CollectingAgentCommitPort {
    fn fail_next_run_start(&self) {
        self.fail_run_starts.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_next_run_terminal(&self) {
        self.fail_run_terminals.fetch_add(1, Ordering::SeqCst);
    }

    fn consume_failure(counter: &AtomicU64) -> bool {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_sub(1)
            })
            .is_ok()
    }
}

#[async_trait]
impl AgentCommitPort for CollectingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        session_id: &str,
        command: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        if matches!(&command, AgentDurableCommand::RunStarted { .. })
            && Self::consume_failure(&self.fail_run_starts)
        {
            return Err(CommitError::Unavailable);
        }
        if matches!(&command, AgentDurableCommand::RunTerminal { .. })
            && Self::consume_failure(&self.fail_run_terminals)
        {
            return Err(CommitError::Unavailable);
        }
        let agent_instance_id = match &command {
            AgentDurableCommand::Create { identity, .. } => identity.agent_instance_id.clone(),
            AgentDurableCommand::SetLifecycle {
                agent_instance_id, ..
            }
            | AgentDurableCommand::ConsumeInboxItem {
                agent_instance_id, ..
            } => agent_instance_id.clone(),
            AgentDurableCommand::RunTerminal { report, .. } => report.agent_instance_id.clone(),
            AgentDurableCommand::RunStarted {
                agent_instance_id, ..
            } => agent_instance_id.clone(),
            AgentDurableCommand::InputQueued {
                agent_instance_id, ..
            }
            | AgentDurableCommand::QueuedInputStarted {
                agent_instance_id, ..
            } => agent_instance_id.clone(),
            AgentDurableCommand::CommitReport {
                recipient_agent_instance_id,
                ..
            } => recipient_agent_instance_id.clone(),
        };
        self.commands.lock().await.push(command);
        Ok(AgentCommitAck {
            session_id: session_id.to_string(),
            agent_instance_id,
            revision: self.revision.fetch_add(1, Ordering::SeqCst) + 1,
        })
    }
}

fn test_agent() -> AgentSpec {
    AgentSpec {
        id: "main".into(),
        name: "main".into(),
        role: "test".into(),
        description: None,
        system_prompt: "test".into(),
        model: Some("faux-1".into()),
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    }
}

async fn attached_runtime() -> (
    AgentRuntime,
    Arc<CollectingAgentCommitPort>,
    Arc<FauxProvider>,
) {
    let model = Arc::new(FauxProvider::new());
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let mut coder = test_agent();
    coder.id = "coder".into();
    coder.name = "coder".into();
    runtime.register_agent(coder).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-1".into(),
            root: AgentInstanceIdentity {
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .expect("attach agent session");
    (runtime, agents, model)
}

#[tokio::test]
async fn root_and_child_are_committed_before_they_are_routable() {
    let (runtime, commits, _model) = attached_runtime().await;
    let child = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-1".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "coder".into(),
            requested_agent_instance_id: Some("coder-1".into()),
            origin_execution_id: Some("exec-parent".into()),
            origin_tool_call_id: Some("call-1".into()),
        })
        .await
        .expect("create child");

    assert_eq!(child.identity.agent_instance_id, "coder-1");
    assert_eq!(
        child.identity.parent_agent_instance_id.as_deref(),
        Some("root")
    );
    let snapshots = runtime
        .list_agents("session-1".into())
        .await
        .expect("list agents");
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].identity.agent_instance_id, "root");
    assert_eq!(snapshots[1].identity.agent_instance_id, "coder-1");

    let commands = commits.commands.lock().await;
    assert_eq!(commands.len(), 2);
    assert!(matches!(commands[0], AgentDurableCommand::Create { .. }));
    assert!(matches!(commands[1], AgentDurableCommand::Create { .. }));
}

#[tokio::test]
async fn failed_run_start_commit_rolls_back_execution_reservation() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("runs after retry").await;
    commits.fail_next_run_start();

    let request = SendAgentInputRequest {
        request_id: "atomic-start-fails".into(),
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        caller_agent_instance_id: None,
        requested_execution_id: Some("exec-atomic-start-fails".into()),
        source_turn_id: None,
        message_id: "message-atomic-start-fails".into(),
        content: MessageContent::String("first attempt".into()),
        delivery: AgentInputDelivery::StartWhenIdle,
    };
    assert!(matches!(
        runtime.run_agent(request).await,
        Err(orchd_api::AgentApiError::PersistenceFailed(_))
    ));
    assert_eq!(model.call_count().await, 0);

    let report = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "atomic-start-retry".into(),
            requested_execution_id: Some("exec-atomic-start-retry".into()),
            message_id: "message-atomic-start-retry".into(),
            content: MessageContent::String("retry".into()),
            ..SendAgentInputRequest {
                request_id: String::new(),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                requested_execution_id: None,
                source_turn_id: None,
                message_id: String::new(),
                content: MessageContent::String(String::new()),
                delivery: AgentInputDelivery::StartWhenIdle,
            }
        })
        .await
        .unwrap();
    assert_eq!(report.summary, "runs after retry");
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn terminal_report_is_not_published_until_retry_commits() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("durable terminal").await;
    commits.fail_next_run_terminal();

    let report = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        runtime.run_agent(SendAgentInputRequest {
            request_id: "terminal-retry".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-terminal-retry".into()),
            source_turn_id: None,
            message_id: "message-terminal-retry".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        }),
    )
    .await
    .expect("terminal persistence retry must be bounded")
    .unwrap();
    assert_eq!(report.summary, "durable terminal");
    assert_eq!(
        commits
            .commands
            .lock()
            .await
            .iter()
            .filter(|command| matches!(command, AgentDurableCommand::RunTerminal { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn failed_message_commit_never_advances_reusable_agent_transcript() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("must not become durable context").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-message-atomicity".into(),
            root: AgentInstanceIdentity {
                session_id: "session-message-atomicity".into(),
                agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                parent_agent_instance_id: None,
            },
            recovered_agents: Vec::new(),
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(Arc::new(FailingMessageCommitPort {
                    attempt: AtomicU64::new(0),
                    fail_at: 2,
                })),
            },
        })
        .await
        .unwrap();

    let report = runtime
        .run_agent(SendAgentInputRequest {
            request_id: "message-atomicity".into(),
            session_id: "session-message-atomicity".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-message-atomicity".into()),
            source_turn_id: None,
            message_id: "message-input-atomicity".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert!(matches!(
        report.outcome,
        piko_protocol::ExecutionOutcome::Failed { .. }
    ));
    assert!(report.summary.is_empty());
}

#[tokio::test]
async fn lifecycle_and_activity_are_independent() {
    let (runtime, _commits, model) = attached_runtime().await;
    let closed = runtime
        .close_agent(AgentLifecycleRequest {
            request_id: "close-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("close root");
    assert_eq!(closed.lifecycle, AgentInstanceLifecycle::Closed);

    let snapshot = runtime
        .agent_snapshot("session-1".into(), "root".into())
        .await
        .expect("snapshot")
        .expect("root snapshot");
    assert_eq!(snapshot.lifecycle, AgentInstanceLifecycle::Closed);
    assert!(matches!(
        snapshot.activity,
        piko_protocol::AgentActivity::Idle
    ));
    let rejected = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "closed-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: None,
            source_turn_id: None,
            message_id: "closed-message".into(),
            content: MessageContent::String("must reject".into()),
            delivery: AgentInputDelivery::Auto,
        })
        .await
        .expect_err("closed AgentInstance must reject input");
    assert_eq!(rejected, orchd_api::AgentApiError::AgentClosed);

    let reopened = runtime
        .reopen_agent(AgentLifecycleRequest {
            request_id: "reopen-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
        })
        .await
        .expect("reopen root");
    assert_eq!(reopened.lifecycle, AgentInstanceLifecycle::Open);
    model.push_text("reused after reopen").await;
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "reopened-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-reopened".into()),
            source_turn_id: None,
            message_id: "reopened-message".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::Auto,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn cross_session_or_missing_parent_is_rejected_before_commit() {
    let (runtime, commits, _model) = attached_runtime().await;
    let error = runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-bad".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "not-in-session".into(),
            agent_spec_id: "coder".into(),
            requested_agent_instance_id: None,
            origin_execution_id: None,
            origin_tool_call_id: None,
        })
        .await
        .expect_err("missing parent must fail");
    assert_eq!(error, orchd_api::AgentApiError::AgentNotFound);
    assert_eq!(commits.commands.lock().await.len(), 1);
}

#[tokio::test]
async fn create_and_input_requests_are_idempotent() {
    let (runtime, commits, model) = attached_runtime().await;
    let create = CreateAgentRequest {
        request_id: "create-idempotent".into(),
        session_id: "session-1".into(),
        parent_agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        requested_agent_instance_id: Some("child-idempotent".into()),
        origin_execution_id: None,
        origin_tool_call_id: None,
    };
    let first = runtime.create_agent(create.clone()).await.unwrap();
    let second = runtime.create_agent(create).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(commits.commands.lock().await.len(), 2, "root + one child");

    model.push_text("one execution").await;
    let input = SendAgentInputRequest {
        request_id: "input-idempotent".into(),
        session_id: "session-1".into(),
        agent_instance_id: "child-idempotent".into(),
        caller_agent_instance_id: Some("root".into()),
        requested_execution_id: None,
        source_turn_id: None,
        message_id: "message-idempotent".into(),
        content: MessageContent::String("run once".into()),
        delivery: AgentInputDelivery::StartWhenIdle,
    };
    let first_report = runtime.run_agent(input.clone()).await.unwrap();
    let duplicate_report = runtime.run_agent(input).await.unwrap();
    assert_eq!(first_report.execution_id, duplicate_report.execution_id);
    assert_eq!(model.call_count().await, 1);
}

#[tokio::test]
async fn sibling_messaging_is_rejected_by_runtime_policy() {
    let (runtime, _commits, _model) = attached_runtime().await;
    for child in ["child-a", "child-b"] {
        runtime
            .create_agent(CreateAgentRequest {
                request_id: format!("create-{child}"),
                session_id: "session-1".into(),
                parent_agent_instance_id: "root".into(),
                agent_spec_id: "main".into(),
                requested_agent_instance_id: Some(child.into()),
                origin_execution_id: None,
                origin_tool_call_id: None,
            })
            .await
            .unwrap();
    }

    let error = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "sibling-message".into(),
            session_id: "session-1".into(),
            agent_instance_id: "child-b".into(),
            caller_agent_instance_id: Some("child-a".into()),
            requested_execution_id: None,
            source_turn_id: None,
            message_id: "sibling-message".into(),
            content: MessageContent::String("not allowed".into()),
            delivery: AgentInputDelivery::Auto,
        })
        .await
        .expect_err("siblings must not acquire arbitrary routing capability");
    assert_eq!(error, orchd_api::AgentApiError::AgentUnauthorized);
}

#[tokio::test]
async fn existing_agent_keeps_resolved_spec_snapshot_after_registry_update() {
    let (runtime, _commits, model) = attached_runtime().await;
    runtime
        .create_agent(CreateAgentRequest {
            request_id: "create-snapshot".into(),
            session_id: "session-1".into(),
            parent_agent_instance_id: "root".into(),
            agent_spec_id: "main".into(),
            requested_agent_instance_id: Some("snapshot-child".into()),
            origin_execution_id: None,
            origin_tool_call_id: None,
        })
        .await
        .unwrap();
    let mut updated = test_agent();
    updated.system_prompt = "updated globally".into();
    runtime.register_agent(updated).await;
    model.push_text("done").await;
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "run-snapshot".into(),
            session_id: "session-1".into(),
            agent_instance_id: "snapshot-child".into(),
            caller_agent_instance_id: Some("root".into()),
            requested_execution_id: Some("exec-snapshot".into()),
            source_turn_id: None,
            message_id: "message-snapshot".into(),
            content: MessageContent::String("run".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert_eq!(model.requests().await[0].system_prompt, "test");
}

#[tokio::test]
async fn follow_up_runs_as_a_later_execution_on_the_same_agent() {
    let (runtime, commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    model.push_text("follow-up run").await;

    let first = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "first-run".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-first".into()),
            source_turn_id: None,
            message_id: "message-first".into(),
            content: MessageContent::String("first".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    let follow_up = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "follow-up-run".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-follow-up".into()),
            source_turn_id: None,
            message_id: "message-follow-up".into(),
            content: MessageContent::String("follow up".into()),
            delivery: AgentInputDelivery::FollowUp,
        })
        .await
        .unwrap();
    assert_eq!(first.execution_id.as_deref(), Some("exec-first"));
    assert_eq!(
        follow_up.disposition,
        piko_protocol::InputDisposition::Queued
    );
    assert!(follow_up.execution_id.is_none());
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::InputQueued { queued_input, .. }
            if queued_input.queued_input_id == "follow-up-run"
    )));
    runtime
        .cancel_agent_run("session-1".into(), "root".into())
        .await
        .unwrap();

    for _ in 0..200 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        if snapshot
            .latest_report
            .as_ref()
            .is_some_and(|report| report.summary == "follow-up run")
            && matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
        {
            assert_eq!(model.call_count().await, 2);
            assert!(commits.commands.lock().await.iter().any(|command| matches!(
                command,
                AgentDurableCommand::QueuedInputStarted { queued_input_id, .. }
                    if queued_input_id == "follow-up-run"
            )));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("follow-up Execution did not complete");
}

#[tokio::test]
async fn agent_reuses_private_transcript_across_executions() {
    let (runtime, commits, model) = attached_runtime().await;
    model.push_text("first answer").await;
    model.push_text("second answer").await;

    for (request_id, message_id, content) in [
        ("input-1", "message-1", "first question"),
        ("input-2", "message-2", "second question"),
    ] {
        runtime
            .send_agent_input(SendAgentInputRequest {
                request_id: request_id.into(),
                session_id: "session-1".into(),
                agent_instance_id: "root".into(),
                caller_agent_instance_id: None,
                requested_execution_id: None,
                source_turn_id: None,
                message_id: message_id.into(),
                content: MessageContent::String(content.into()),
                delivery: AgentInputDelivery::StartWhenIdle,
            })
            .await
            .expect("start agent execution");
        wait_until_idle(&runtime).await;
    }

    let requests = model.requests().await;
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1].transcript.iter().any(|message| matches!(
            message,
            piko_protocol::Message::Assistant { content, .. }
                if content.iter().any(|block| matches!(
                    block,
                    piko_protocol::ContentBlock::Text { text } if text == "first answer"
                ))
        )),
        "second Execution must receive the first Execution's private transcript"
    );
    assert!(commits.commands.lock().await.iter().any(|command| matches!(
        command,
        AgentDurableCommand::RunTerminal { report, .. }
            if report.summary == "second answer"
    )));
}

async fn wait_until_idle(runtime: &AgentRuntime) {
    for _ in 0..100 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .expect("snapshot")
            .expect("root");
        if matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
            && snapshot.latest_report.is_some()
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("AgentActor did not return to Idle");
}

#[tokio::test]
async fn multi_agent_tools_use_trusted_context_for_attached_and_detached_spawn() {
    let (runtime, commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model.push_text("attached report").await;
    model.push_text("detached report").await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let context = ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-exec".into(),
        cancellation: None,
        agent_id: "main".into(),
        tool_set_ids: Vec::new(),
        turn_index: Some(1),
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
    };

    let attached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-attached".into(),
                name: "spawn_agent".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do attached work",
                    "session_id": "forged-session",
                    "parent_agent_instance_id": "forged-parent"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(attached.ok, "attached spawn failed: {:?}", attached.error);
    assert_eq!(
        attached.value.as_ref().unwrap()["summary"],
        "attached report"
    );
    assert!(
        attached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    let detached = provider
        .execute(
            piko_protocol::ToolCall {
                id: "call-detached".into(),
                name: "spawn_agent_detached".into(),
                arguments: serde_json::json!({
                    "agent_spec_id": "main",
                    "prompt": "do detached work"
                }),
                partial_json: None,
            },
            context.clone(),
        )
        .await;
    assert!(detached.ok, "detached spawn failed: {:?}", detached.error);
    assert_eq!(detached.value.as_ref().unwrap()["status"], "accepted");
    assert!(
        detached
            .value
            .as_ref()
            .unwrap()
            .get("execution_id")
            .is_none()
    );

    for _ in 0..100 {
        let inbox = runtime
            .agent_inbox("session-1".into(), "root".into())
            .await
            .expect("root inbox");
        if inbox
            .items
            .iter()
            .any(|item| item.report.summary == "detached report")
        {
            assert!(commits.commands.lock().await.iter().any(|command| matches!(
                command,
                AgentDurableCommand::CommitReport { report, .. }
                    if report.summary == "detached report"
            )));
            let collected = provider
                .execute(
                    piko_protocol::ToolCall {
                        id: "call-collect".into(),
                        name: "collect_agent_reports".into(),
                        arguments: serde_json::json!({}),
                        partial_json: None,
                    },
                    context,
                )
                .await;
            assert!(collected.ok);
            assert_eq!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]["summary"],
                "detached report"
            );
            assert!(
                collected.value.as_ref().unwrap()["reports"][0]["report"]
                    .get("execution_id")
                    .is_none()
            );
            let consumed = runtime
                .agent_inbox("session-1".into(), "root".into())
                .await
                .unwrap();
            assert!(consumed.items[0].consumed_at.is_some());
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("detached report was not delivered to the durable parent inbox");
}

#[tokio::test]
async fn recovered_child_restores_private_transcript_and_inbox() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("after recovery").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    let child = AgentInstanceIdentity {
        session_id: "session-recovery".into(),
        agent_instance_id: "child".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: Some("root".into()),
    };
    let old_report = piko_protocol::AgentExecutionReport {
        agent_instance_id: "child".into(),
        execution_id: "exec-old".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "old answer".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![
                AgentRecoveryState {
                    identity: root,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: Vec::new(),
                    head_message_id: None,
                    inbox: vec![piko_protocol::AgentInboxItem {
                        report_id: "report-old".into(),
                        recipient_agent_instance_id: "root".into(),
                        source_agent_instance_id: "child".into(),
                        report: old_report.clone(),
                        committed_at: 1,
                        consumed_at: None,
                    }],
                    latest_report: None,
                    execution_reports: Vec::new(),
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
                AgentRecoveryState {
                    identity: child,
                    spec: test_agent(),
                    lifecycle: AgentInstanceLifecycle::Open,
                    transcript: vec![
                        piko_protocol::Message::User {
                            content: MessageContent::String("before recovery".into()),
                            timestamp: Some(1),
                        },
                        piko_protocol::Message::Assistant {
                            content: vec![piko_protocol::ContentBlock::Text {
                                text: "old answer".into(),
                            }],
                            api: "test".into(),
                            provider: "test".into(),
                            model: "test".into(),
                            usage: None,
                            stop_reason: Some("stop".into()),
                            error_message: None,
                            timestamp: Some(2),
                        },
                    ],
                    head_message_id: Some("old-head".into()),
                    inbox: Vec::new(),
                    latest_report: Some(old_report),
                    execution_reports: vec![piko_protocol::AgentExecutionReport {
                        agent_instance_id: "child".into(),
                        execution_id: "exec-old".into(),
                        outcome: piko_protocol::ExecutionOutcome::Succeeded {
                            usage: Default::default(),
                        },
                        summary: "old answer".into(),
                        usage: Default::default(),
                        artifacts: Vec::new(),
                    }],
                    queued_inputs: Vec::new(),
                    pending_detached_deliveries: Vec::new(),
                },
            ],
            ports: SessionAgentPorts {
                agents: agents as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    let inbox = runtime
        .agent_inbox("session-recovery".into(), "root".into())
        .await
        .unwrap();
    assert_eq!(inbox.items.len(), 1);
    let duplicate = runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "replayed-old-execution".into(),
            session_id: "session-recovery".into(),
            agent_instance_id: "child".into(),
            caller_agent_instance_id: Some("root".into()),
            requested_execution_id: Some("exec-old".into()),
            source_turn_id: None,
            message_id: "replayed-old-message".into(),
            content: MessageContent::String("must not rerun".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert_eq!(
        duplicate.disposition,
        piko_protocol::InputDisposition::Duplicate
    );
    assert_eq!(model.call_count().await, 0);
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "after-recovery".into(),
            session_id: "session-recovery".into(),
            agent_instance_id: "child".into(),
            caller_agent_instance_id: Some("root".into()),
            requested_execution_id: Some("exec-new".into()),
            source_turn_id: None,
            message_id: "message-new".into(),
            content: MessageContent::String("continue".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    assert!(
        model.requests().await[0]
            .transcript
            .iter()
            .any(|message| matches!(
                message,
                piko_protocol::Message::Assistant { content, .. }
                    if content.iter().any(|block| matches!(
                        block,
                        piko_protocol::ContentBlock::Text { text } if text == "old answer"
                    ))
            ))
    );
}

#[tokio::test]
async fn recovered_durable_follow_up_starts_without_new_input() {
    let model = Arc::new(FauxProvider::new());
    model.push_text("recovered follow-up").await;
    let runtime = AgentRuntime::new(model.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let agents = Arc::new(CollectingAgentCommitPort::default());
    let executions = Arc::new(CollectingExecutionCommitPort::new());
    let root = AgentInstanceIdentity {
        session_id: "session-queued-recovery".into(),
        agent_instance_id: "root".into(),
        agent_spec_id: "main".into(),
        parent_agent_instance_id: None,
    };
    runtime
        .attach_agent_session(SessionAgentConfig {
            session_id: "session-queued-recovery".into(),
            root: root.clone(),
            recovered_agents: vec![AgentRecoveryState {
                identity: root,
                spec: test_agent(),
                lifecycle: AgentInstanceLifecycle::Open,
                transcript: Vec::new(),
                head_message_id: None,
                inbox: Vec::new(),
                latest_report: None,
                execution_reports: Vec::new(),
                queued_inputs: vec![piko_protocol::DurableAgentInput {
                    queued_input_id: "queued-recovery".into(),
                    request: SendAgentInputRequest {
                        request_id: "queued-recovery".into(),
                        session_id: "session-queued-recovery".into(),
                        agent_instance_id: "root".into(),
                        caller_agent_instance_id: None,
                        requested_execution_id: Some("exec-queued-recovery".into()),
                        source_turn_id: None,
                        message_id: "message-queued-recovery".into(),
                        content: MessageContent::String("continue".into()),
                        delivery: AgentInputDelivery::FollowUp,
                    },
                    detached_recipient_agent_instance_id: None,
                }],
                pending_detached_deliveries: Vec::new(),
            }],
            ports: SessionAgentPorts {
                agents: agents.clone() as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions as Arc<dyn orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    for _ in 0..200 {
        let snapshot = runtime
            .agent_snapshot("session-queued-recovery".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        if snapshot
            .latest_report
            .as_ref()
            .is_some_and(|report| report.summary == "recovered follow-up")
        {
            assert!(agents.commands.lock().await.iter().any(|command| matches!(
                command,
                AgentDurableCommand::QueuedInputStarted { queued_input_id, .. }
                    if queued_input_id == "queued-recovery"
            )));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("recovered durable follow-up did not start");
}

#[tokio::test]
async fn cancelling_attached_spawn_cancels_child_execution() {
    let (runtime, _commits, model) = attached_runtime().await;
    let runtime = Arc::new(runtime);
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    let provider = MultiAgentToolProvider::new(runtime.clone() as Arc<dyn AgentRuntimeApi>);
    let cancellation = tokio_util::sync::CancellationToken::new();
    let context = ToolExecutionContext {
        session_id: "session-1".into(),
        agent_instance_id: "root".into(),
        execution_id: "parent-cancel".into(),
        cancellation: Some(cancellation.clone()),
        agent_id: "main".into(),
        tool_set_ids: Vec::new(),
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: None,
        tool_entity_id: None,
        host_context: None,
        source_turn_id: None,
    };
    let spawned = tokio::spawn(async move {
        provider
            .execute(
                piko_protocol::ToolCall {
                    id: "call-cancel".into(),
                    name: "spawn_agent".into(),
                    arguments: serde_json::json!({
                        "agent_spec_id": "main",
                        "prompt": "wait"
                    }),
                    partial_json: None,
                },
                context,
            )
            .await
    });
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    cancellation.cancel();
    let result = spawned.await.unwrap();
    assert!(!result.ok);
    assert_eq!(result.error.unwrap().message, "operation cancelled");

    for _ in 0..100 {
        let child = runtime
            .list_agents("session-1".into())
            .await
            .unwrap()
            .into_iter()
            .find(|snapshot| snapshot.identity.parent_agent_instance_id.as_deref() == Some("root"))
            .unwrap();
        if let Some(report) = child.latest_report {
            assert!(matches!(
                report.outcome,
                piko_protocol::ExecutionOutcome::Cancelled { .. }
            ));
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("cancelled child did not reach a terminal report");
}

#[tokio::test]
async fn session_detach_cancels_and_drains_active_executions() {
    let (runtime, _commits, model) = attached_runtime().await;
    model
        .push_response(faux_provider::CannedResponse::waiting_for_cancel())
        .await;
    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "shutdown-input".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            requested_execution_id: Some("exec-shutdown".into()),
            source_turn_id: None,
            message_id: "message-shutdown".into(),
            content: MessageContent::String("wait".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
        })
        .await
        .unwrap();
    for _ in 0..100 {
        if model.call_count().await == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        runtime.detach_agent_session("session-1".into()),
    )
    .await
    .expect("detach must be bounded")
    .expect("detach must drain");
    assert_eq!(
        runtime.list_agents("session-1".into()).await.unwrap_err(),
        orchd_api::AgentApiError::SessionNotAttached
    );
}
