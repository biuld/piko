//! F-07 approval decision mapping tests (registry gate).

use async_trait::async_trait;

use crate::adapters::tools::registry::{CatalogRoute, ToolRegistry, ToolRegistryImpl};
use crate::domain::tools::approval::{ToolApprovalDecision, ToolApprovalRequest};
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolExecutorRef,
};
use crate::ports::approval_gateway::ApprovalGateway;
use piko_orchd_api::tools::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use piko_orchd_api::{ToolExecResult, is_approval_accepted};
use piko_protocol::messages::ToolCall;
use piko_protocol::tools::ToolProviderSource;

const TOOL_NAME: &str = "needs_approval";

fn approved_tool() -> ToolDef {
    ToolDef {
        name: TOOL_NAME.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", TOOL_NAME),
        description: String::new(),
        input_schema: serde_json::json!({}),
        executor: ToolExecutorRef {
            kind: "native".into(),
            target: TOOL_NAME.into(),
            extra: None,
        },
        execution_mode: Some(ToolExecutionMode::Sequential),
        exposure: None,
        capabilities: None,
        approval: Some(ToolApprovalRequirement::OnRequest),
        metadata: None,
    }
}

fn route() -> CatalogRoute {
    CatalogRoute {
        provider_id: "fake".into(),
        provider_tool_name: TOOL_NAME.into(),
        tool_def: approved_tool(),
        execution_mode: ToolExecutionMode::Sequential,
        max_concurrent_calls: None,
    }
}

fn context() -> ToolExecutionContext {
    ToolExecutionContext {
        session_id: "session".into(),
        agent_instance_id: "agent_session_root".into(),
        execution_id: "exec".into(),
        cancellation: None,
        agent_id: "root".into(),
        tool_set_ids: vec![],
        turn_index: None,
        event_seq: None,
        next_event_seq: None,
        parent_message_id: None,
        content_index: None,
        tool_call_index: Some(0),
        tool_entity_id: Some("entity-1".into()),
        host_context: None,
        source_turn_id: None,
    }
}

struct FakeProvider;

#[async_trait]
impl ToolProvider for FakeProvider {
    fn id(&self) -> &str {
        "fake"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Orch
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![approved_tool()]
    }

    async fn execute(&self, _call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        ToolExecResult {
            ok: true,
            value: Some(serde_json::json!({ "ran": true })),
            error: None,
        }
    }
}

#[derive(Clone)]
struct StubApprovalGateway(ToolApprovalDecision);

#[async_trait]
impl ApprovalGateway for StubApprovalGateway {
    async fn request_tool_approval(&self, _request: ToolApprovalRequest) -> ToolApprovalDecision {
        self.0.clone()
    }
}

async fn registry_with_gateway(decision: Option<ToolApprovalDecision>) -> ToolRegistryImpl {
    let registry = ToolRegistryImpl::new();
    registry.register_provider(Box::new(FakeProvider)).await;
    registry
        .set_approval_gateway(
            decision.map(|d| Box::new(StubApprovalGateway(d)) as Box<dyn ApprovalGateway>),
        )
        .await;
    registry
}

fn call() -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        name: TOOL_NAME.into(),
        arguments: serde_json::json!({}),
        partial_json: None,
    }
}

#[tokio::test]
async fn expired_decision_fails_closed_with_distinct_error() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::Expired)).await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("expired approval must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "approval_expired");
    assert_eq!(error.retryable, Some(false));
    assert!(error.message.contains("expired"));
}

#[tokio::test]
async fn declined_decision_keeps_distinct_error() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::Decline)).await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("declined approval must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "declined");
    assert_eq!(error.retryable, Some(false));
}

#[tokio::test]
async fn accepted_decision_runs_the_tool() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::Accept)).await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    assert!(record.result.ok);
    assert_eq!(
        record.result.value,
        Some(serde_json::json!({ "ran": true }))
    );
}

#[tokio::test]
async fn no_gateway_denies_tools_requiring_approval() {
    let registry = registry_with_gateway(None).await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("missing gateway must deny");
    assert_eq!(error.code, "approval_unavailable");
    assert_eq!(error.retryable, Some(false));
}

#[test]
fn expired_is_never_accepted() {
    assert!(!is_approval_accepted(&ToolApprovalDecision::Expired));
    assert!(!is_approval_accepted(&ToolApprovalDecision::Decline));
    assert!(is_approval_accepted(&ToolApprovalDecision::Accept));
    assert!(is_approval_accepted(&ToolApprovalDecision::AcceptSession));
}
