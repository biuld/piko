//! Vertical-slice tests for the single-agent Execution path.

#[path = "common/faux_provider.rs"]
mod faux_provider;

use std::sync::Arc;

use orchd::testing::{AgentExecutionRuntime, CollectingExecutionCommitPort};
use orchd_api::{
    AgentExecutor, ConversationContext, ExecutionConfig, ExecutionOutcome, ExecutionStatus,
    SessionExecutionConfig, SessionExecutionPorts, StartExecutionRequest,
};
use piko_protocol::MessageContent;
use piko_protocol::agents::AgentSpec;

use faux_provider::{CannedResponse, CannedToolCall, FauxProvider};

fn test_agent() -> AgentSpec {
    AgentSpec {
        id: "main".into(),
        name: "main".into(),
        role: "test".into(),
        description: None,
        system_prompt: "You are a test agent.".into(),
        model: Some("faux-1".into()),
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    }
}

#[tokio::test]
async fn start_execution_completes_text_only_step() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("hello from execution").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-exec-1".into(),
            ports: SessionExecutionPorts::new(
                sink.clone() as Arc<dyn orchd_api::ExecutionCommitPort>
            ),
        })
        .await
        .expect("attach");

    let receipt = runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-1".into(),
            session_id: "session-exec-1".into(),
            source_turn_id: Some("turn-1".into()),
            execution_id: "exec-1".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-user-1".into(),
            input: MessageContent::String("hi".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                model: Some("faux-1".into()),
                provider: Some("faux".into()),
                allow_tool_calls: false,
            },
        })
        .await
        .expect("start");

    assert_eq!(receipt.status, ExecutionStatus::Accepted);
    assert_eq!(receipt.execution_id, "exec-1");

    let outcome = runtime
        .wait_terminal("session-exec-1", "exec-1")
        .await
        .expect("terminal");
    assert!(matches!(outcome, ExecutionOutcome::Succeeded { .. }));

    let messages = sink.messages();
    assert!(
        messages
            .iter()
            .any(|m| matches!(&m.message, piko_protocol::Message::Assistant { .. })),
        "expected assistant message commit; got {messages:?}"
    );

    // Active handle should be removed after finalization.
    let snapshot = runtime
        .execution_snapshot("session-exec-1".into(), "exec-1".into())
        .await
        .expect("snapshot");
    assert!(snapshot.is_none(), "handle should be reaped");
}

#[tokio::test]
async fn request_cancel_finalizes_cancelled_outcome() {
    let faux = Arc::new(FauxProvider::new());
    // No canned response pushed — cancel before/during the model wait.
    // Push a response anyway so if cancel loses the race we still terminate.
    faux.push_text("should cancel").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-cancel".into(),
            ports: SessionExecutionPorts::new(
                sink.clone() as Arc<dyn orchd_api::ExecutionCommitPort>
            ),
        })
        .await
        .expect("attach");

    runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-cancel".into(),
            session_id: "session-cancel".into(),
            source_turn_id: Some("turn-cancel".into()),
            execution_id: "exec-cancel".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-cancel".into(),
            input: MessageContent::String("cancel me".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                ..Default::default()
            },
        })
        .await
        .expect("start");

    let receipt = runtime
        .request_cancel(orchd_api::CancelExecutionRequest {
            request_id: "req-cancel-cmd".into(),
            session_id: "session-cancel".into(),
            execution_id: "exec-cancel".into(),
            reason: orchd_api::CancelReason::UserRequested,
        })
        .await
        .expect("cancel accepted");
    assert!(receipt.accepted);

    let outcome = runtime
        .wait_terminal("session-cancel", "exec-cancel")
        .await
        .expect("terminal");

    // Cancel may race a fast faux response; either Cancelled or Succeeded is
    // acceptable for this slice as long as exactly one terminal is committed.
    assert!(
        matches!(
            outcome,
            ExecutionOutcome::Cancelled { .. } | ExecutionOutcome::Succeeded { .. }
        ),
        "unexpected {outcome:?}"
    );
    assert_eq!(sink.outcomes().len(), 1);
}

#[tokio::test]
async fn single_agent_rejects_second_concurrent_execution() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first").await;
    faux.push_text("second").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-exec-2".into(),
            ports: SessionExecutionPorts::new(sink as Arc<dyn orchd_api::ExecutionCommitPort>),
        })
        .await
        .expect("attach");

    runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-a".into(),
            session_id: "session-exec-2".into(),
            source_turn_id: Some("turn-a".into()),
            execution_id: "exec-a".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-a".into(),
            input: MessageContent::String("a".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                ..Default::default()
            },
        })
        .await
        .expect("first start");

    let second = runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-b".into(),
            session_id: "session-exec-2".into(),
            source_turn_id: Some("turn-b".into()),
            execution_id: "exec-b".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-b".into(),
            input: MessageContent::String("b".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                ..Default::default()
            },
        })
        .await;

    match second {
        Err(orchd_api::AgentApiError::ExecutionAlreadyActive) => {}
        Ok(_) => {
            // First may have already reaped; sequential starts are allowed.
        }
        Err(other) => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn distinct_agent_instances_can_execute_concurrently_in_one_session() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("root result").await;
    faux.push_text("child result").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;
    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-multi".into(),
            ports: SessionExecutionPorts::new(sink as Arc<dyn orchd_api::ExecutionCommitPort>),
        })
        .await
        .expect("attach");

    for (execution_id, agent_instance_id) in [("exec-root", "root"), ("exec-child", "child-1")] {
        runtime
            .start_execution(StartExecutionRequest {
                request_id: format!("request-{execution_id}"),
                session_id: "session-multi".into(),
                source_turn_id: Some(format!("turn-{execution_id}")),
                execution_id: execution_id.into(),
                agent_instance_id: agent_instance_id.into(),
                agent_spec: test_agent(),
                input_message_id: format!("message-{execution_id}"),
                input: MessageContent::String("run".into()),
                context: ConversationContext::empty(),
                config: ExecutionConfig {
                    agent_id: "main".into(),
                    ..Default::default()
                },
            })
            .await
            .expect("different AgentInstance must not conflict");
    }

    runtime
        .wait_terminal("session-multi", "exec-root")
        .await
        .expect("root terminal");
    runtime
        .wait_terminal("session-multi", "exec-child")
        .await
        .expect("child terminal");
}

#[tokio::test]
async fn start_execution_continues_after_tool_batch() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_response(CannedResponse::with_tools(
        "need a tool",
        vec![CannedToolCall {
            id: "call_missing".into(),
            name: "missing_tool".into(),
            arguments: serde_json::json!({"path": "nope"}),
        }],
    ))
    .await;
    faux.push_text("done after tool").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-tools".into(),
            ports: SessionExecutionPorts::new(
                sink.clone() as Arc<dyn orchd_api::ExecutionCommitPort>
            ),
        })
        .await
        .expect("attach");

    runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-tools".into(),
            session_id: "session-tools".into(),
            source_turn_id: Some("turn-tools".into()),
            execution_id: "exec-tools".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-tools".into(),
            input: MessageContent::String("use tool".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                allow_tool_calls: true,
                ..Default::default()
            },
        })
        .await
        .expect("start");

    let outcome = runtime
        .wait_terminal("session-tools", "exec-tools")
        .await
        .expect("terminal");
    assert!(matches!(outcome, ExecutionOutcome::Succeeded { .. }));

    let messages = sink.messages();
    assert!(
        messages.iter().any(|m| matches!(
            &m.message,
            piko_protocol::Message::ToolCall { id, .. } if id == "call_missing"
        )),
        "expected tool call commit; got {messages:?}"
    );
    assert!(
        messages.iter().any(|m| matches!(
            &m.message,
            piko_protocol::Message::ToolResult {
                tool_call_id,
                is_error: Some(true),
                ..
            } if tool_call_id == "call_missing"
        )),
        "expected tool error result; got {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(&m.message, piko_protocol::Message::Assistant { .. })),
        "expected assistant messages; got {messages:?}"
    );
    assert_eq!(sink.outcomes().len(), 1);
}

/// LLM gateway that waits for a release signal before returning the next canned stream.
struct GatedFaux {
    inner: FauxProvider,
    gate: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>>,
}

#[async_trait::async_trait]
impl llmd::gateway::LlmGateway for GatedFaux {
    async fn chat_stream(
        &self,
        req: llmd::gateway::GatewayRequest,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures_core::Stream<Item = llmd::gateway::GatewayEvent> + Send + 'static>,
        >,
        String,
    > {
        if let Some(rx) = self.gate.lock().await.take() {
            let _ = rx.await;
        }
        self.inner.chat_stream(req, cancel).await
    }

    async fn llm_call(
        &self,
        model: piko_protocol::messages::Model,
        system_prompt: Option<String>,
        messages: Vec<piko_protocol::Message>,
        settings: piko_protocol::model::ModelRunSettings,
    ) -> Result<String, String> {
        self.inner
            .llm_call(model, system_prompt, messages, settings)
            .await
    }

    fn capabilities(&self) -> piko_protocol::model::ModelCapabilities {
        self.inner.capabilities()
    }
}

#[tokio::test]
async fn steer_execution_is_committed_before_next_model_step() {
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let faux = Arc::new(GatedFaux {
        inner: FauxProvider::new(),
        gate: Arc::new(tokio::sync::Mutex::new(Some(release_rx))),
    });
    // First response after release; second after steering is injected.
    faux.inner.push_text("first").await;
    faux.inner.push_text("second after steer").await;

    let runtime = AgentExecutionRuntime::new(faux.clone() as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-steer".into(),
            ports: SessionExecutionPorts::new(
                sink.clone() as Arc<dyn orchd_api::ExecutionCommitPort>
            ),
        })
        .await
        .expect("attach");

    runtime
        .start_execution(StartExecutionRequest {
            request_id: "req-steer".into(),
            session_id: "session-steer".into(),
            source_turn_id: Some("turn-steer".into()),
            execution_id: "exec-steer".into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: "msg-steer-user".into(),
            input: MessageContent::String("start".into()),
            context: ConversationContext::empty(),
            config: ExecutionConfig {
                agent_id: "main".into(),
                allow_tool_calls: false,
                ..Default::default()
            },
        })
        .await
        .expect("start");

    // Actor is blocked in the first model step gate — steer should queue.
    let receipt = runtime
        .steer_execution(orchd_api::SteerExecutionRequest {
            request_id: "req-steer-input".into(),
            session_id: "session-steer".into(),
            execution_id: "exec-steer".into(),
            message_id: "msg-steer".into(),
            content: MessageContent::String("steering note".into()),
            submitted_at: 42,
        })
        .await
        .expect("steer");
    assert_eq!(receipt.disposition, orchd_api::InputDisposition::Queued);

    let _ = release_tx.send(());

    let outcome = runtime
        .wait_terminal("session-steer", "exec-steer")
        .await
        .expect("terminal");
    assert!(matches!(outcome, ExecutionOutcome::Succeeded { .. }));

    assert!(
        sink.messages().iter().any(|m| {
            matches!(
                &m.message,
                piko_protocol::Message::User {
                    content: MessageContent::String(text),
                    ..
                } if text == "steering note"
            )
        }),
        "expected steering user message; got {:?}",
        sink.messages()
    );
}

#[tokio::test]
async fn sequential_turns_use_distinct_executions() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("turn one").await;
    faux.push_text("turn two").await;

    let runtime = AgentExecutionRuntime::new(faux as Arc<dyn llmd::gateway::LlmGateway>);
    runtime.register_agent(test_agent()).await;

    let sink = Arc::new(CollectingExecutionCommitPort::new());
    runtime
        .attach_session(SessionExecutionConfig {
            session_id: "session-seq".into(),
            ports: SessionExecutionPorts::new(
                sink.clone() as Arc<dyn orchd_api::ExecutionCommitPort>
            ),
        })
        .await
        .expect("attach");

    let start = |turn: &str, exec: &str, msg: &str, input: &str, prior_head: Option<&str>| {
        StartExecutionRequest {
            request_id: format!("req-{turn}"),
            session_id: "session-seq".into(),
            source_turn_id: Some(turn.into()),
            execution_id: exec.into(),
            agent_instance_id: "root".into(),
            agent_spec: test_agent(),
            input_message_id: msg.into(),
            input: MessageContent::String(input.into()),
            context: ConversationContext {
                messages: Vec::new(),
                head_message_id: prior_head.map(str::to_string),
                system_prompt: None,
            },
            config: ExecutionConfig {
                agent_id: "main".into(),
                model: Some("faux-1".into()),
                provider: Some("faux".into()),
                allow_tool_calls: false,
            },
        }
    };

    runtime
        .start_execution(start("turn-1", "exec-1", "msg-1", "first", None))
        .await
        .expect("start 1");
    let o1 = runtime
        .wait_terminal("session-seq", "exec-1")
        .await
        .expect("terminal 1");
    assert!(matches!(o1, ExecutionOutcome::Succeeded { .. }));

    // Prior-turn head in context must not become the assistant parent of turn 2.
    runtime
        .start_execution(start(
            "turn-2",
            "exec-2",
            "msg-2",
            "second",
            Some("exec-1:step_1:assistant"),
        ))
        .await
        .expect("start 2");
    let o2 = runtime
        .wait_terminal("session-seq", "exec-2")
        .await
        .expect("terminal 2");
    assert!(matches!(o2, ExecutionOutcome::Succeeded { .. }));

    assert_eq!(sink.outcomes().len(), 2);
    let messages = sink.messages();
    let turn2_assistant = messages.iter().find(|m| {
        m.execution_id == "exec-2" && matches!(m.message, piko_protocol::Message::Assistant { .. })
    });
    assert_eq!(
        turn2_assistant.and_then(|m| m.parent_message_id.as_deref()),
        Some("msg-2"),
        "assistant must parent the turn's input message, not prior execution head"
    );
    let texts: Vec<_> = messages
        .iter()
        .filter_map(|m| match &m.message {
            piko_protocol::Message::Assistant { content, .. } => {
                content.iter().find_map(|b| match b {
                    piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"turn one"));
    assert!(texts.contains(&"turn two"));
}
