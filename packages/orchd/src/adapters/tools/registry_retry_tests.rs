use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolExecutorRef,
};
use crate::ports::approval_gateway::ApprovalGateway;
use piko_orchd_api::tools::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use piko_orchd_api::{ToolApprovalDecision, ToolApprovalRequest, ToolExecError, ToolExecResult};
use piko_protocol::messages::ToolCall;
use piko_protocol::tools::ToolProviderSource;

struct DenyOnceProvider {
    calls: Arc<AtomicUsize>,
    deny_message: String,
}

#[async_trait]
impl ToolProvider for DenyOnceProvider {
    fn id(&self) -> &str {
        "fake-exec"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Workspace
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![]
    }

    async fn execute(&self, call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "sandbox_denied".into(),
                    message: self.deny_message.clone(),
                    retryable: Some(true),
                }),
            }
        } else {
            ToolExecResult {
                ok: true,
                value: Some(serde_json::json!({
                    "authority": call.arguments["sandbox_permissions"]
                })),
                error: None,
            }
        }
    }
}

struct CapturingGateway(Arc<Mutex<Vec<ToolApprovalRequest>>>);

#[async_trait]
impl ApprovalGateway for CapturingGateway {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
        self.0.lock().await.push(request);
        ToolApprovalDecision::Accept
    }
}

fn definition() -> ToolDef {
    ToolDef {
        name: "exec_command".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "exec"),
        description: String::new(),
        input_schema: serde_json::json!({}),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: "exec_command".into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Sequential),
        exposure: None,
        capabilities: None,
        approval: Some(ToolApprovalRequirement::Never),
        metadata: None,
    }
}

fn context() -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: "session".into(),
        agent_instance_id: "agent".into(),
        execution_id: "execution".into(),
        cancellation: None,
        agent_id: "root".into(),
        agent_role: None,
        agent_kind: piko_protocol::AgentKind::Supervisor,
        tool_set_ids: vec![],
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: Some(0),
        tool_entity_id: Some("entity".into()),
        host_context: None,
        source_turn_id: None,
        context_remaining: None,
    }
}

#[tokio::test]
async fn sandbox_denial_gets_one_approved_elevated_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::clone(&calls),
            deny_message: "outside default roots".into(),
        }))
        .await;
    registry
        .set_approval_gateway(Some(Box::new(CapturingGateway(Arc::clone(&captured)))))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "pwd" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    assert!(record.result.ok, "{:?}", record.result);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let requests = captured.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].tool_args["sandbox_permissions"],
        "require_escalated"
    );
}

#[tokio::test]
async fn sandbox_denial_retries_with_narrow_additional_read_permissions() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::clone(&calls),
            deny_message: "sandbox-exec: deny file-read-data /opt/homebrew/bin/magick".into(),
        }))
        .await;
    registry
        .set_approval_gateway(Some(Box::new(CapturingGateway(Arc::clone(&captured)))))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "git status --short" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    assert!(record.result.ok, "{:?}", record.result);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let requests = captured.lock().await;
    assert_eq!(requests.len(), 1);
    let retry_args = &requests[0].tool_args;
    assert_eq!(
        retry_args["sandbox_permissions"],
        "with_additional_permissions"
    );
    assert_eq!(
        retry_args["additional_permissions"]["read_roots"][0],
        "/opt/homebrew/bin/magick"
    );
    assert!(retry_args["justification"].as_str().is_some());
    // Simple program + subcommand retries carry a reusable narrow prefix.
    assert_eq!(
        retry_args["prefix_rule"],
        serde_json::json!(["git", "status"])
    );
}

#[tokio::test]
async fn sandbox_denial_skips_prefix_for_complex_shell() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::clone(&calls),
            deny_message: "sandbox-exec: deny file-read-data /Users/biu/.gitconfig".into(),
        }))
        .await;
    registry
        .set_approval_gateway(Some(Box::new(CapturingGateway(Arc::clone(&captured)))))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "cd /tmp && git status" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    assert!(record.result.ok, "{:?}", record.result);
    let requests = captured.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].tool_args["sandbox_permissions"],
        "with_additional_permissions"
    );
    assert!(requests[0].tool_args.get("prefix_rule").is_none());
}

#[tokio::test]
async fn write_denial_retries_with_write_roots_and_ancestor() {
    let calls = Arc::new(AtomicUsize::new(0));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::clone(&calls),
            deny_message: "sandbox denied: deny write /tmp/piko-f34-missing/child".into(),
        }))
        .await;
    registry
        .set_approval_gateway(Some(Box::new(CapturingGateway(Arc::clone(&captured)))))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "mkdir /tmp/piko-f34-missing/child" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    assert!(record.result.ok, "{:?}", record.result);
    let retry_args = &captured.lock().await[0].tool_args;
    let writes = retry_args["additional_permissions"]["write_roots"]
        .as_array()
        .unwrap();
    assert!(writes.iter().any(|v| v == "/tmp/piko-f34-missing/child"));
    assert!(writes.iter().any(|v| v == "/tmp"));
}

#[tokio::test]
async fn no_gateway_appends_escalation_guidance() {
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::new(AtomicUsize::new(0)),
            deny_message: "sandbox denied: deny write /opt/x".into(),
        }))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "mkdir /opt/x" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    let error = record.result.error.unwrap();
    assert_eq!(error.code, "sandbox_denied");
    assert!(
        error
            .message
            .contains("approval-backed retry is unavailable")
    );
}

#[tokio::test]
async fn approved_prefix_retry_reports_grant() {
    let registry = ToolRegistryImpl::new();
    registry
        .register_provider(Box::new(DenyOnceProvider {
            calls: Arc::new(AtomicUsize::new(0)),
            deny_message: "sandbox denied: deny write".into(),
        }))
        .await;
    registry
        .set_approval_gateway(Some(Box::new(CapturingGateway(Arc::new(Mutex::new(
            Vec::new(),
        ))))))
        .await;
    let route = CatalogRoute {
        provider_id: "fake-exec".into(),
        provider_tool_name: "exec_command".into(),
        tool_def: definition(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    };
    let call = ToolCall {
        id: "call".into(),
        name: "exec_command".into(),
        arguments: serde_json::json!({ "cmd": "brew install jq" }),
        partial_json: None,
    };
    let record = registry.execute_tool(&call, &context(), &route, None).await;
    assert!(record.result.ok, "{:?}", record.result);
    let grant = &record.result.value.unwrap()["approved_grant"];
    assert_eq!(grant["prefix"], serde_json::json!(["brew", "install"]));
    assert!(grant["note"].as_str().unwrap().contains("reuse"));
}
