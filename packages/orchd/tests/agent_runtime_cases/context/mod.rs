// ---- F-04/F-05 context-management: model-view truncation + budget tools ----

// ---- F-04 context-management: model-view truncation acceptance ----

use piko_orchd_api::{ToolDiscoveryContext, ToolExecError, ToolExecResult};
use piko_protocol::messages::{ContentBlock, Message, ToolCall};
use piko_protocol::tools::{ToolSet, ToolSetToolRef};
use piko_protocol::{
    ToolApprovalRequirement, ToolDef, ToolExecutorRef, ToolProviderSource,
};

/// Returns a fixed oversized payload for every call.
#[derive(Clone)]
struct BloatProvider {
    payload: String,
}

impl BloatProvider {
    fn new(chars: usize) -> Self {
        Self {
            payload: "z".repeat(chars),
        }
    }
}

#[async_trait]
impl ToolProvider for BloatProvider {
    fn id(&self) -> &str {
        "bloat"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![ToolDef {
            name: "bloat_emit".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "bloat/bloat_emit"),
            description: "Emit a large text payload.".into(),
            input_schema: serde_json::json!({ "type": "object" }),
            executor: ToolExecutorRef {
                kind: "bloat".into(),
                target: "bloat_emit".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: Some(ToolApprovalRequirement::Never),
            metadata: None,
        }]
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        if call.name == "bloat_emit" {
            ToolExecResult {
                ok: true,
                value: Some(serde_json::json!(self.payload)),
                error: None,
            }
        } else {
            ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "unknown_tool".into(),
                    message: "unknown tool".into(),
                    retryable: None,
                }),
            }
        }
    }
}

#[tokio::test]
async fn oversized_tool_output_is_truncated_in_model_view_but_kept_in_committed_transcript() {
    let model = Arc::new(FauxProvider::new());

    let mut agents = std::collections::HashMap::new();
    let mut agent = test_agent();
    agent.tool_set_ids = vec!["bloat".into()];
    agents.insert("main".into(), agent);

    let mut config = piko_protocol::config::OrchdConfig::single_provider("faux", "test", "faux-1");
    config.agents = agents;
    let runtime = AgentRuntime::bootstrap(
        model.clone() as Arc<dyn piko_llmd::gateway::LlmGateway>,
        config,
    )
    .await;
    runtime
        .register_tool_provider(Box::new(BloatProvider::new(200_000)))
        .await;
    runtime
        .register_tool_set(ToolSet {
            id: "bloat".into(),
            name: "Bloat".into(),
            description: None,
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "bloat".into(),
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
            session_id: "session-context".into(),
            root: AgentInstanceIdentity {
                session_id: "session-context".into(),
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

    model
        .push_response(CannedResponse::tool_calls(vec![ToolCall {
            id: "call-bloat".into(),
            name: "bloat_emit".into(),
            arguments: serde_json::json!({}),
            partial_json: None,
        }]))
        .await;
    model.push_text("done").await;
    runtime
        .run_agent(SendAgentInputRequest {
            request_id: "context-run".into(),
            session_id: "session-context".into(),
            agent_instance_id: "root".into(),
            caller_agent_instance_id: None,
            source_turn_id: None,
            message_id: "context-message".into(),
            content: MessageContent::String("run".into()),
            delivery: piko_protocol::AgentInputDelivery::Auto,
            prompt_resources: None,
            active_tool_names: None,
        })
        .await
        .unwrap();

    let requests = model.requests().await;
    assert!(requests.len() >= 2, "expected at least two model requests");
    let second = &requests[1];
    let marker = second
        .transcript
        .iter()
        .find_map(|message| match message {
            Message::ToolResult { content, .. } => content.iter().find_map(|block| match block {
                ContentBlock::Text { text } if text.contains("Tool output truncated") => {
                    Some(text.clone())
                }
                _ => None,
            }),
            _ => None,
        })
        .expect("second model request must contain a truncation marker");
    assert!(marker.contains("retained"), "{marker}");
    assert!(marker.contains("of 200000 characters"), "{marker}");
    assert!(
        !marker.contains(&"z".repeat(200_000)),
        "marker must not include the full payload"
    );

    // The committed transcript (what hostd persists) keeps the full output.
    let commits = executions.messages();
    let full = commits.iter().find_map(|commit| match &commit.message {
        Message::ToolResult {
            tool_name,
            content,
            ..
        } if tool_name.as_deref() == Some("bloat_emit") => content.iter().find_map(|block| {
            if let ContentBlock::Text { text } = block {
                Some(text.clone())
            } else {
                None
            }
        }),
        _ => None,
    });
    assert_eq!(
        full.as_deref(),
        Some("z".repeat(200_000).as_str()),
        "committed transcript must retain the full tool output"
    );
}

include!("budget_tools.rs");
include!("truncation_cap.rs");
