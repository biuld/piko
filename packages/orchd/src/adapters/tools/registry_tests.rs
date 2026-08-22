//! F-07 approval decision mapping tests (registry gate).

use std::collections::HashMap;

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
use piko_protocol::tools::{ToolProviderSource, ToolSet, ToolSetToolRef};

mod model_surface;
mod policy_denials;

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
        agent_role: None,
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
        context_remaining: None,
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
struct StubApprovalGateway {
    decision: ToolApprovalDecision,
    captured: Option<std::sync::Arc<tokio::sync::Mutex<Vec<ToolApprovalRequest>>>>,
}

#[async_trait]
impl ApprovalGateway for StubApprovalGateway {
    async fn request_tool_approval(&self, request: ToolApprovalRequest) -> ToolApprovalDecision {
        if let Some(captured) = &self.captured {
            captured.lock().await.push(request);
        }
        self.decision.clone()
    }
}

async fn registry_with_gateway(decision: Option<ToolApprovalDecision>) -> ToolRegistryImpl {
    let registry = ToolRegistryImpl::new();
    registry.register_provider(Box::new(FakeProvider)).await;
    registry
        .set_approval_gateway(decision.map(|d| {
            Box::new(StubApprovalGateway {
                decision: d,
                captured: None,
            }) as Box<dyn ApprovalGateway>
        }))
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

// ---- F-18 managed-feature gating tests ----

fn catalog_tool(name: &str, executor_kind: &str) -> ToolDef {
    ToolDef {
        name: name.into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", name),
        description: String::new(),
        input_schema: serde_json::json!({}),
        executor: ToolExecutorRef {
            kind: executor_kind.into(),
            target: name.into(),
            extra: None,
        },
        execution_mode: None,
        exposure: None,
        capabilities: None,
        approval: None,
        metadata: None,
    }
}

struct CatalogProvider;

#[async_trait]
impl ToolProvider for CatalogProvider {
    fn id(&self) -> &str {
        "catalog"
    }

    fn source(&self) -> ToolProviderSource {
        ToolProviderSource::Workspace
    }

    async fn discover(&self, _context: ToolDiscoveryContext) -> Vec<ToolDef> {
        vec![
            catalog_tool("read", "native"),
            catalog_tool("exec_command", "native"),
            catalog_tool("write_stdin", "native"),
            catalog_tool("environment", "native"),
            catalog_tool("get_context_remaining", "native"),
            catalog_tool("todo_write", "native"),
            catalog_tool("spawn_agent", "native"),
            catalog_tool("ask_user", "native"),
            // MCP tools have server-defined names and an mcp executor kind.
            catalog_tool("server_defined_tool", "mcp"),
        ]
    }

    async fn execute(&self, _call: ToolCall, _context: ToolExecutionContext) -> ToolExecResult {
        ToolExecResult {
            ok: true,
            value: None,
            error: None,
        }
    }
}

fn discovery_context(active_tool_names: Option<Vec<String>>) -> ToolDiscoveryContext {
    ToolDiscoveryContext {
        agent_id: "main".into(),
        agent_instance_id: Some("agent_session_root".into()),
        tool_set_ids: vec!["workspace".into()],
        active_tool_names,
    }
}

async fn catalog_registry(features: Option<HashMap<String, bool>>) -> ToolRegistryImpl {
    let registry = ToolRegistryImpl::new();
    registry.register_provider(Box::new(CatalogProvider)).await;
    registry
        .register_tool_set(ToolSet {
            id: "workspace".into(),
            name: "Workspace".into(),
            description: None,
            feature: Some(piko_protocol::tools::ToolSetFeature::ByTool {
                tool_features: HashMap::from([
                    ("read".into(), "workspace".into()),
                    ("exec_command".into(), "exec".into()),
                    ("write_stdin".into(), "exec".into()),
                    ("environment".into(), "environment".into()),
                    ("get_context_remaining".into(), "context".into()),
                    ("todo_write".into(), "todo".into()),
                    ("spawn_agent".into(), "multi-agent".into()),
                    ("ask_user".into(), "user-interaction".into()),
                    ("server_defined_tool".into(), "mcp".into()),
                ]),
            }),
            metadata: None,
            policy: None,
            tools: vec![ToolSetToolRef::ProviderNamespace {
                provider_id: "catalog".into(),
                namespace: String::new(),
                alias: None,
                policy: None,
            }],
        })
        .await;
    registry.set_features(features).await;
    registry
}

fn contribution_set(id: &str, provider_id: &str) -> ToolSet {
    ToolSet {
        id: id.into(),
        name: id.into(),
        description: None,
        feature: None,
        metadata: None,
        policy: None,
        tools: vec![ToolSetToolRef::ProviderNamespace {
            provider_id: provider_id.into(),
            namespace: String::new(),
            alias: None,
            policy: None,
        }],
    }
}

#[tokio::test]
async fn contribution_registration_is_atomic_and_never_replaces_ids() {
    let registry = ToolRegistryImpl::new();
    registry
        .install_contribution(super::registry::ToolContribution {
            provider: Box::new(CatalogProvider),
            tool_sets: vec![contribution_set("catalog-set", "catalog")],
        })
        .await
        .unwrap();
    let error = registry
        .install_contribution(super::registry::ToolContribution {
            provider: Box::new(CatalogProvider),
            tool_sets: vec![contribution_set("replacement-set", "catalog")],
        })
        .await
        .unwrap_err();
    assert!(error.contains("already registered"));
    let sets = registry.list_tool_sets().await;
    assert!(sets.contains_key("catalog-set"));
    assert!(!sets.contains_key("replacement-set"));
}

#[tokio::test]
async fn invalid_contribution_publishes_nothing() {
    let registry = ToolRegistryImpl::new();
    let error = registry
        .install_contribution(super::registry::ToolContribution {
            provider: Box::new(CatalogProvider),
            tool_sets: vec![contribution_set("broken", "different-provider")],
        })
        .await
        .unwrap_err();
    assert!(error.contains("references provider"));
    assert!(registry.list_tool_sets().await.is_empty());
}

fn feature_map(entries: &[(&str, bool)]) -> HashMap<String, bool> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), *value))
        .collect()
}

#[tokio::test]
async fn no_feature_map_keeps_the_full_catalog() {
    let registry = catalog_registry(None).await;
    let (tools, routes) = registry
        .discover_tools(&discovery_context(None))
        .await
        .expect("catalog builds");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"exec_command"));
    assert!(names.contains(&"write_stdin"));
    assert!(names.contains(&"server_defined_tool"));
    assert!(routes.contains_key("write_stdin"));
    assert!(routes.contains_key("server_defined_tool"));
}

#[tokio::test]
async fn disabled_features_remove_tools_and_routes() {
    let registry = catalog_registry(Some(feature_map(&[("exec", false), ("mcp", false)]))).await;
    let (tools, routes) = registry
        .discover_tools(&discovery_context(None))
        .await
        .expect("catalog builds");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(!names.contains(&"write_stdin"));
    assert!(!names.contains(&"server_defined_tool"));
    assert!(!names.contains(&"exec_command"));
    assert!(names.contains(&"read"));
    assert!(!routes.contains_key("write_stdin"));
    assert!(!routes.contains_key("server_defined_tool"));
    assert!(!routes.contains_key("exec_command"));
}

#[tokio::test]
async fn feature_gate_classifies_direct_calls() {
    let registry = catalog_registry(Some(feature_map(&[("exec", false)]))).await;
    assert_eq!(
        registry.feature_gate("exec_command").await.as_deref(),
        Some("exec")
    );
    assert_eq!(
        registry.feature_gate("write_stdin").await.as_deref(),
        Some("exec")
    );
    assert_eq!(registry.feature_gate("unknown_tool").await, None);
    // Server-defined MCP tool names cannot be classified by name alone.
    assert_eq!(registry.feature_gate("server_defined_tool").await, None);
}

#[tokio::test]
async fn active_tool_names_still_intersect_with_features() {
    let registry = catalog_registry(Some(feature_map(&[("exec", false)]))).await;
    let (tools, routes) = registry
        .discover_tools(&discovery_context(Some(vec![
            "read".into(),
            "exec_command".into(),
        ])))
        .await
        .expect("catalog builds");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["read"]);
    assert!(routes.contains_key("read"));
    assert!(!routes.contains_key("exec_command"));
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
async fn approval_request_carries_executing_agent_role() {
    let registry = ToolRegistryImpl::new();
    registry.register_provider(Box::new(FakeProvider)).await;
    let captured = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    registry
        .set_approval_gateway(Some(Box::new(StubApprovalGateway {
            decision: ToolApprovalDecision::Accept,
            captured: Some(std::sync::Arc::clone(&captured)),
        })))
        .await;

    let mut ctx = context();
    ctx.agent_role = Some("researcher".into());
    let record = registry.execute_tool(&call(), &ctx, &route(), None).await;
    assert!(record.result.ok);

    let requests = captured.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].agent_role.as_deref(), Some("researcher"));
    assert_eq!(requests[0].agent_id, "root");
    // F-13: the catalog route's provider id is stamped so hostd can resolve
    // `server/tool` approval templates for MCP tools.
    assert_eq!(requests[0].provider_id.as_deref(), Some("fake"));
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

#[tokio::test]
async fn guardian_denied_decision_fails_closed_with_reason() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::GuardianDenied {
        reason: "outside workspace".into(),
    }))
    .await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("guardian denial must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "guardian_denied");
    assert_eq!(error.retryable, Some(false));
    assert!(error.message.contains("outside workspace"));
}

#[tokio::test]
async fn guardian_unavailable_decision_fails_closed() {
    let registry = registry_with_gateway(Some(ToolApprovalDecision::GuardianUnavailable)).await;
    let record = registry
        .execute_tool(&call(), &context(), &route(), None)
        .await;

    let error = record.result.error.expect("guardian failure must fail");
    assert!(!record.result.ok);
    assert_eq!(error.code, "guardian_unavailable");
    assert_eq!(error.retryable, Some(false));
}
