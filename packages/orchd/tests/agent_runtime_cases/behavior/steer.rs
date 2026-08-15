use super::*;

use piko_orchd_api::{ToolDiscoveryContext, ToolExecResult};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};
use piko_protocol::{ToolApprovalRequirement, ToolDef, ToolExecutorRef, ToolProviderSource};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A tool that stays inside `execute` until the test releases it, so the
/// turn is deterministically mid-flight when the steer arrives.
#[derive(Clone)]
struct BlockingToolProvider {
    started: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
}

impl BlockingToolProvider {
    fn new() -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            release: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl piko_orchd_api::ToolProvider for BlockingToolProvider {
    fn id(&self) -> &str {
        "block"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![ToolDef {
            name: "block_until_released".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "block/block_until_released"),
            description: "Block until the test releases the gate.".into(),
            input_schema: serde_json::json!({ "type": "object" }),
            executor: ToolExecutorRef {
                kind: "block".into(),
                target: "block_until_released".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: Some(ToolApprovalRequirement::Never),
            metadata: None,
        }]
    }

    async fn execute(&self, _call: piko_protocol::ToolCall, _context: piko_orchd_api::ToolExecutionContext) -> ToolExecResult {
        self.started.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({ "released": true })),
            error: None,
        }
    }
}

#[tokio::test]
async fn steered_message_is_answered_before_further_tool_work() {
    let model = Arc::new(FauxProvider::new());

    let mut agents = std::collections::HashMap::new();
    let mut agent = test_agent();
    agent.tool_set_ids = vec!["block".into()];
    agents.insert("main".into(), agent);

    let mut config = test_orchd_config();
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::InferenceGateway>,
        config,
    )
    .await;
    let blocker = BlockingToolProvider::new();
    runtime
        .register_tool_provider(Box::new(blocker.clone()))
        .await;
    runtime
        .register_tool_set(ToolSet {
            id: "block".into(),
            name: "Block".into(),
            description: None,
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "block".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        })
        .await;

    let agents_port = Arc::new(CollectingAgentCommitPort::default());
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
                agents: agents_port as Arc<dyn AgentCommitPort>,
                executions: SessionExecutionPorts::new(
                    executions.clone() as Arc<dyn piko_orchd_api::ExecutionCommitPort>,
                ),
            },
        })
        .await
        .unwrap();

    // Step 1: a tool-calling step keeps the turn alive.
    model
        .push_response(faux_provider::CannedResponse::tool_calls(vec![
            piko_protocol::ToolCall {
                id: "call-1".into(),
                name: "block_until_released".into(),
                arguments: serde_json::json!({}),
                partial_json: None,
            },
        ]))
        .await;
    // Step 2: the respond-only answer to the steered message.
    model.push_text("最新情况：正在调查中").await;
    // Step 3: the resumed normal step ends the turn.
    model.push_text("done").await;

    runtime
        .send_agent_input(SendAgentInputRequest {
            request_id: "run-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-1".into(),
            content: MessageContent::String("investigate".into()),
            delivery: AgentInputDelivery::StartWhenIdle,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    // Wait until the blocking tool is executing: the turn is deterministically
    // mid-flight and can be steered.
    let started_result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while !blocker.started.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await;
    if started_result.is_err() {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        panic!(
            "blocking tool must start; call_count={} activity={:?} report={:?}",
            model.call_count().await,
            snapshot.activity,
            snapshot.latest_report
        );
    }
    assert_eq!(model.call_count().await, 1, "first model step must have run");

    // Steer a user message into the running turn. The receipt only arrives
    // after the blocking tool completes and the run loop drains the mailbox,
    // so send it from a background task and release the tool afterwards.
    let runtime = Arc::new(runtime);
    let steer_runtime = Arc::clone(&runtime);
    let steer_task = tokio::spawn(async move {
        steer_runtime
            .send_agent_input(SendAgentInputRequest {
            request_id: "steer-1".into(),
            session_id: "session-1".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "message-steer".into(),
            content: MessageContent::String("汇报一下情况".into()),
            delivery: AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
            .await
    });
    // Give the steer a moment to reach the execution mailbox, then release
    // the tool so the boundary drain commits the steer.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    blocker.release.store(true, Ordering::SeqCst);
    let steer = steer_task.await.unwrap().unwrap();
    assert_eq!(
        steer.disposition,
        piko_protocol::InputDisposition::Queued,
        "steer into a running turn queues until the next model-step boundary"
    );

    for _ in 0..1000 {
        let snapshot = runtime
            .agent_snapshot("session-1".into(), "root".into())
            .await
            .unwrap()
            .unwrap();
        if matches!(snapshot.activity, piko_protocol::AgentActivity::Idle)
            && model.call_count().await == 3
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let requests = model.requests().await;
    assert_eq!(
        requests.len(),
        3,
        "tool step, respond step, and resumed step must all run"
    );
    // The respond-only step must disable tools and carry the reply
    // instruction (F-35 / ADR-021).
    assert_eq!(
        requests[1].options.tool_choice,
        piko_llmd::gateway::ToolChoice::None
    );
    assert!(
        gateway_prompt_text(&requests[1]).contains("Answer that message directly now"),
        "respond step must carry the steer-reply instruction"
    );
    // Steps before and after the respond step keep tools enabled.
    assert_ne!(
        requests[0].options.tool_choice,
        piko_llmd::gateway::ToolChoice::None
    );
    assert_ne!(
        requests[2].options.tool_choice,
        piko_llmd::gateway::ToolChoice::None
    );
}
