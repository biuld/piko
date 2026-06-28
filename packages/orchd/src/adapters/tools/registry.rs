// ---- ToolRegistryImpl — DI container for tools ----
//
// This is a service called directly by the agent runtime.
// Responsibilities:
//   - Hold references to all registered providers, tool_sets, approval gateway
//   - discover_tools(): pure computation over shared state
//   - execute_tool(): execute a tool on a provider, applying policy, approvals
//
// Writes (registerProvider etc.) are synchronous mutations on shared Maps
// protected by tokio::sync::RwLock.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::domain::tools::approval::{ToolApprovalDecision, ToolApprovalRequest};
use crate::domain::tools::call::ContentBlock;
use crate::domain::tools::definition::{
    ToolApprovalPolicy, ToolApprovalRequirement, ToolDef, ToolPolicy, ToolSensitivity, ToolSet,
    ToolSetPolicy, ToolSetToolRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::approval_gateway::ApprovalGateway;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use crate::runtime::agent_stream::tool_executor::runtime_tool_entity_id;
use piko_protocol::Event;

// ---- CatalogRoute ----

/// Route from public tool name to the provider that implements it.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogRoute {
    pub provider_id: String,
    pub provider_tool_name: String,
    pub tool_def: ToolDef,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionRecord {
    pub result: ToolExecResult,
    pub events: Vec<Event>,
}

// ---- ToolRegistry trait ----

/// Public interface for tool discovery and execution.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Discover tools available for the given context.
    async fn discover_tools(
        &self,
        context: &ToolDiscoveryContext,
    ) -> (Vec<ToolDef>, HashMap<String, CatalogRoute>);

    /// Execute a tool call through its registered provider.
    ///
    /// `call` should be `ContentBlock::ToolCall { .. }` — other variants will
    /// produce an immediate error result.
    async fn execute_tool(
        &self,
        call: &ContentBlock,
        context: &ToolExecutionContext,
        route: &CatalogRoute,
        cancel: Option<CancellationToken>,
    ) -> ToolExecutionRecord;
}

// ---- ToolRegistryImpl ----

pub struct ToolRegistryImpl {
    providers: RwLock<HashMap<String, Box<dyn ToolProvider>>>,
    tool_sets: RwLock<HashMap<String, ToolSet>>,
    approval_gateway: RwLock<Option<Box<dyn ApprovalGateway>>>,
}

impl ToolRegistryImpl {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            tool_sets: RwLock::new(HashMap::new()),
            approval_gateway: RwLock::new(None),
        }
    }

    // ---- Singleton registration ----

    /// Register a tool provider.
    pub async fn register_provider(&self, provider: Box<dyn ToolProvider>) {
        let id = provider.id().to_string();
        self.providers.write().await.insert(id, provider);
    }

    /// Unregister a tool provider by ID.
    pub async fn unregister_provider(&self, provider_id: &str) {
        self.providers.write().await.remove(provider_id);
    }

    /// Register a tool set.
    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        self.tool_sets
            .write()
            .await
            .insert(tool_set.id.clone(), tool_set);
    }

    /// Unregister a tool set by ID.
    pub async fn unregister_tool_set(&self, tool_set_id: &str) {
        self.tool_sets.write().await.remove(tool_set_id);
    }

    /// Set (or clear) the approval gateway.
    pub async fn set_approval_gateway(&self, gateway: Option<Box<dyn ApprovalGateway>>) {
        *self.approval_gateway.write().await = gateway;
    }

    /// List all registered tool sets.
    pub async fn list_tool_sets(&self) -> std::collections::HashMap<String, ToolSet> {
        self.tool_sets.read().await.clone()
    }

    // ---- Catalog building ----

    /// Build the full tool catalog from registered providers and tool sets.
    async fn build_catalog(
        &self,
        context: &ToolDiscoveryContext,
    ) -> Result<Vec<CatalogEntry>, String> {
        let providers = self.providers.read().await;
        let tool_sets = self.tool_sets.read().await;

        let mut entries: Vec<CatalogEntry> = vec![];
        let mut seen: HashSet<String> = HashSet::new();
        let mut duplicates: HashSet<String> = HashSet::new();
        let mut provider_cache: HashMap<String, Vec<ToolDef>> = HashMap::new();

        // Helper: discover tools from a provider (with caching).
        async fn discover_from<'a>(
            provider_id: &str,
            cache: &mut HashMap<String, Vec<ToolDef>>,
            providers: &tokio::sync::RwLockReadGuard<'a, HashMap<String, Box<dyn ToolProvider>>>,
            ctx: &ToolDiscoveryContext,
        ) -> Vec<ToolDef> {
            if let Some(cached) = cache.get(provider_id) {
                return cached.clone();
            }
            if let Some(p) = providers.get(provider_id) {
                let tools = p
                    .discover(ToolDiscoveryContext {
                        agent_id: ctx.agent_id.clone(),
                        task_id: ctx.task_id.clone(),
                        tool_set_ids: vec![],
                        active_tool_names: None,
                    })
                    .await;
                cache.insert(provider_id.to_string(), tools.clone());
                return tools;
            }
            vec![]
        }

        // Process each tool set reference
        for tool_set in tool_sets.values() {
            if !context.tool_set_ids.contains(&tool_set.id) {
                continue;
            }

            for tool_ref in &tool_set.tools {
                let policy = merge_policy(tool_set.policy.as_ref(), tool_ref_policy(tool_ref));

                match tool_ref {
                    ToolSetToolRef::ProviderTool {
                        provider_id,
                        tool_name,
                        alias,
                        ..
                    } => {
                        let tools =
                            discover_from(provider_id, &mut provider_cache, &providers, context)
                                .await;
                        if let Some(td) = tools.iter().find(|t| t.name == *tool_name) {
                            let public_name = alias.as_ref().unwrap_or(tool_name);
                            add_entry(
                                &mut entries,
                                &mut seen,
                                &mut duplicates,
                                public_name,
                                provider_id,
                                tool_name,
                                td,
                                policy.as_ref(),
                            );
                        }
                    }
                    ToolSetToolRef::OrchestratorControl { action, alias, .. } => {
                        let tools =
                            discover_from("orch", &mut provider_cache, &providers, context).await;
                        if let Some(td) = tools.iter().find(|t| t.name == *action) {
                            let public_name = alias.as_ref().unwrap_or(action);
                            add_entry(
                                &mut entries,
                                &mut seen,
                                &mut duplicates,
                                public_name,
                                "orch",
                                action,
                                td,
                                policy.as_ref(),
                            );
                        }
                    }
                    ToolSetToolRef::ProviderNamespace {
                        provider_id,
                        namespace,
                        alias,
                        ..
                    } => {
                        let tools =
                            discover_from(provider_id, &mut provider_cache, &providers, context)
                                .await;
                        for td in &tools {
                            if td.name.starts_with(namespace.as_str()) {
                                let base_name = &td.name[namespace.len()..];
                                let public_name = if let Some(a) = alias {
                                    format!("{a}{base_name}")
                                } else {
                                    td.name.clone()
                                };
                                add_entry(
                                    &mut entries,
                                    &mut seen,
                                    &mut duplicates,
                                    &public_name,
                                    provider_id,
                                    &td.name,
                                    td,
                                    policy.as_ref(),
                                );
                            }
                        }
                    }
                }
            }
        }

        if !duplicates.is_empty() {
            let mut dup_list: Vec<_> = duplicates.iter().cloned().collect();
            dup_list.sort();
            return Err(format!(
                "Duplicate tool names in catalog: {}",
                dup_list.join(", ")
            ));
        }

        Ok(entries)
    }
}

#[async_trait]
impl ToolRegistry for ToolRegistryImpl {
    /// Discover tools: build catalog, apply filter, return (tools, routes).
    async fn discover_tools(
        &self,
        context: &ToolDiscoveryContext,
    ) -> (Vec<ToolDef>, HashMap<String, CatalogRoute>) {
        let catalog = match self.build_catalog(context).await {
            Ok(c) => c,
            Err(_) => return (vec![], HashMap::new()),
        };

        // Apply active tool name restrictions
        let tools: Vec<ToolDef> = if let Some(ref active) = context.active_tool_names {
            catalog
                .iter()
                .filter(|e| active.contains(&e.public_name))
                .map(|e| e.tool_def.clone())
                .collect()
        } else {
            catalog.iter().map(|e| e.tool_def.clone()).collect()
        };

        // Build route map for fast lookup during execution
        let mut routes = HashMap::new();
        for entry in &catalog {
            // If active filter active, only include filtered tools
            if let Some(ref active) = context.active_tool_names
                && !active.contains(&entry.public_name)
            {
                continue;
            }
            routes.insert(
                entry.public_name.clone(),
                CatalogRoute {
                    provider_id: entry.provider_id.clone(),
                    provider_tool_name: entry.provider_tool_name.clone(),
                    tool_def: entry.tool_def.clone(),
                },
            );
        }

        (tools, routes)
    }

    /// Execute a tool call with approval and lifecycle events.
    async fn execute_tool(
        &self,
        call: &ContentBlock,
        context: &ToolExecutionContext,
        route: &CatalogRoute,
        cancel: Option<CancellationToken>,
    ) -> ToolExecutionRecord {
        // Only handle ToolCall content blocks
        let (call_id, call_name, call_args) = match call {
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => (id.clone(), name.clone(), arguments.clone()),
            _ => {
                return ToolExecutionRecord {
                    events: vec![],
                    result: ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "invalid_call".into(),
                            message: "execute_tool requires a ToolCall content block".into(),
                            retryable: Some(false),
                        }),
                    },
                };
            }
        };

        // Compute ordering metadata
        let tool_entity_id = context.tool_entity_id.clone().unwrap_or_else(|| {
            runtime_tool_entity_id(
                context.parent_message_id.as_deref().unwrap_or(""),
                context.tool_call_index.unwrap_or(0),
            )
        });

        let mut events = vec![Event::ToolStart {
            task_id: context.task_id.clone(),
            agent_id: context.agent_id.clone(),
            tool_call_id: call_id.clone(),
            tool_name: call_name.clone(),
            args: call_args.clone(),
            parent_message_id: context.parent_message_id.clone(),
        }];

        // ---- Check cancellation ----
        if let Some(ref token) = cancel
            && token.is_cancelled()
        {
            let result = ToolExecResult {
                ok: false,
                value: None,
                error: Some(ToolExecError {
                    code: "aborted".into(),
                    message: "Task cancelled".into(),
                    retryable: Some(false),
                }),
            };
            Self::push_tool_finished(
                context,
                &call_id,
                &call_name,
                &tool_entity_id,
                &result,
                &mut events,
            );
            return ToolExecutionRecord { result, events };
        }

        // ---- Look up provider ----
        let providers = self.providers.read().await;
        let provider = match providers.get(&route.provider_id) {
            Some(p) => p,
            None => {
                let result = ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "not_found".into(),
                        message: format!(
                            "No provider \"{}\" for tool \"{}\"",
                            route.provider_id, call_name
                        ),
                        retryable: Some(false),
                    }),
                };
                Self::push_tool_finished(
                    context,
                    &call_id,
                    &call_name,
                    &tool_entity_id,
                    &result,
                    &mut events,
                );
                return ToolExecutionRecord { result, events };
            }
        };

        // ---- Approval check ----
        let effective_approval = route
            .tool_def
            .approval
            .clone()
            .unwrap_or(ToolApprovalRequirement::Never);

        if effective_approval != ToolApprovalRequirement::Never {
            let needs_approval = matches!(
                effective_approval,
                ToolApprovalRequirement::Always | ToolApprovalRequirement::OnRequest
            );

            if needs_approval {
                if let Some(ref token) = cancel
                    && token.is_cancelled()
                {
                    let result = ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "aborted".into(),
                            message: "Task cancelled".into(),
                            retryable: Some(false),
                        }),
                    };
                    Self::push_tool_finished(
                        context,
                        &call_id,
                        &call_name,
                        &tool_entity_id,
                        &result,
                        &mut events,
                    );
                    return ToolExecutionRecord { result, events };
                }

                let gateway = self.approval_gateway.read().await;
                if let Some(gw) = gateway.as_ref() {
                    events.push(Event::ApprovalRequested {
                        task_id: context.task_id.clone(),
                        agent_id: context.agent_id.clone(),
                        approval_id: tool_entity_id.clone(),
                        tool_name: call_name.clone(),
                        tool_args: call_args.clone(),
                    });

                    // Race approval against cancellation
                    let approval_request = ToolApprovalRequest {
                        tool_entity_id: tool_entity_id.clone(),
                        call_id: call_id.clone(),
                        agent_id: context.agent_id.clone(),
                        task_id: context.task_id.clone(),
                        tool_name: call_name.clone(),
                        tool_args: call_args.clone(),
                    };

                    let decision = if let Some(token) = cancel {
                        tokio::select! {
                            d = gw.request_tool_approval(approval_request) => d,
                            _ = token.cancelled() => ToolApprovalDecision::Decline,
                        }
                    } else {
                        gw.request_tool_approval(approval_request).await
                    };

                    let host_decision = map_approval_decision(&decision);
                    events.push(Event::ApprovalResolved {
                        task_id: context.task_id.clone(),
                        agent_id: context.agent_id.clone(),
                        approval_id: tool_entity_id.clone(),
                        decision: host_decision,
                    });

                    if matches!(decision, ToolApprovalDecision::Decline) {
                        let result = ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "declined".into(),
                                message: "User declined approval".into(),
                                retryable: Some(false),
                            }),
                        };
                        Self::push_tool_finished(
                            context,
                            &call_id,
                            &call_name,
                            &tool_entity_id,
                            &result,
                            &mut events,
                        );
                        return ToolExecutionRecord { result, events };
                    }
                } else {
                    // No approval gateway configured — deny tools that need approval.
                    let result = ToolExecResult {
                        ok: false,
                        value: None,
                        error: Some(ToolExecError {
                            code: "approval_unavailable".into(),
                            message: format!(
                                "Tool '{call_name}' requires approval but no ApprovalGateway is configured"
                            ),
                            retryable: Some(false),
                        }),
                    };
                    Self::push_tool_finished(
                        context,
                        &call_id,
                        &call_name,
                        &tool_entity_id,
                        &result,
                        &mut events,
                    );
                    return ToolExecutionRecord { result, events };
                }
            }
        }

        // ---- Execute provider ----
        let provider_call = if route.provider_tool_name != call_name {
            ContentBlock::ToolCall {
                id: call_id.clone(),
                name: route.provider_tool_name.clone(),
                arguments: call_args.clone(),
                partial_json: None,
            }
        } else {
            call.clone()
        };

        let exec_context = ToolExecutionContext {
            agent_id: context.agent_id.clone(),
            task_id: context.task_id.clone(),
            tool_set_ids: context.tool_set_ids.clone(),
            turn_index: context.turn_index,
            event_seq: context.event_seq,
            next_event_seq: context.next_event_seq,
            parent_message_id: context.parent_message_id.clone(),
            content_index: context.content_index,
            tool_call_index: context.tool_call_index,
            tool_entity_id: Some(tool_entity_id.clone()),
            host_context: context.host_context.clone(),
        };

        let exec_result = provider.execute(provider_call, exec_context).await;

        // ---- Emit tool_finished ----
        Self::push_tool_finished(
            context,
            &call_id,
            &call_name,
            &tool_entity_id,
            &exec_result,
            &mut events,
        );

        ToolExecutionRecord {
            result: exec_result,
            events,
        }
    }
}

// ---- Private helpers ----

impl ToolRegistryImpl {
    fn push_tool_finished(
        context: &ToolExecutionContext,
        call_id: &str,
        tool_name: &str,
        _tool_entity_id: &str,
        result: &ToolExecResult,
        events: &mut Vec<Event>,
    ) {
        let output = if let Some(ref v) = result.value {
            v.clone()
        } else if let Some(ref e) = result.error {
            serde_json::json!({"error": {"code": e.code, "message": e.message}})
        } else {
            serde_json::Value::Null
        };
        events.push(Event::ToolEnd {
            task_id: context.task_id.clone(),
            agent_id: context.agent_id.clone(),
            tool_call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            result: output,
            is_error: !result.ok,
        });
    }
}

// ---- Free functions ----

#[derive(Debug, Clone)]
struct CatalogEntry {
    public_name: String,
    provider_id: String,
    provider_tool_name: String,
    tool_def: ToolDef,
}

fn add_entry(
    entries: &mut Vec<CatalogEntry>,
    seen: &mut HashSet<String>,
    duplicates: &mut HashSet<String>,
    public_name: &str,
    provider_id: &str,
    provider_tool_name: &str,
    tool_def: &ToolDef,
    policy: Option<&ToolPolicy>,
) {
    if seen.contains(public_name) {
        duplicates.insert(public_name.to_string());
    }
    seen.insert(public_name.to_string());
    entries.push(CatalogEntry {
        public_name: public_name.to_string(),
        provider_id: provider_id.to_string(),
        provider_tool_name: provider_tool_name.to_string(),
        tool_def: project_tool_def(tool_def, public_name, policy),
    });
}

/// Apply policy overrides to a tool definition.
fn project_tool_def(tool_def: &ToolDef, public_name: &str, policy: Option<&ToolPolicy>) -> ToolDef {
    let mut projected = tool_def.clone();
    projected.name = public_name.to_string();

    let Some(p) = policy else {
        return projected;
    };

    // Apply approval policy
    if let Some(ref approval_policy) = p.approval {
        projected.approval = match approval_policy {
            ToolApprovalPolicy::Never => Some(ToolApprovalRequirement::Never),
            ToolApprovalPolicy::OnSensitive => {
                // Keep existing if set, otherwise on_request
                if projected.approval.is_none() {
                    Some(ToolApprovalRequirement::OnRequest)
                } else {
                    projected.approval
                }
            }
            ToolApprovalPolicy::Always => Some(ToolApprovalRequirement::Always),
        };
    } else if let Some(ref sensitivity) = p.sensitivity {
        projected.approval = match sensitivity {
            ToolSensitivity::Safe => projected.approval,
            ToolSensitivity::Sensitive
                if projected
                    .approval
                    .as_ref()
                    .is_some_and(|a| *a == ToolApprovalRequirement::Never) =>
            {
                Some(ToolApprovalRequirement::OnRequest)
            }
            ToolSensitivity::Dangerous => Some(ToolApprovalRequirement::Always),
            ToolSensitivity::Dynamic => projected.approval,
            _ => projected.approval,
        };
    }

    // Apply execution mode
    if let Some(ref mode) = p.execution_mode {
        projected.execution_mode = Some(mode.clone());
    }

    projected
}

/// Extract policy from a tool set reference.
fn tool_ref_policy(tool_ref: &ToolSetToolRef) -> Option<&ToolPolicy> {
    match tool_ref {
        ToolSetToolRef::ProviderTool { policy, .. }
        | ToolSetToolRef::ProviderNamespace { policy, .. }
        | ToolSetToolRef::OrchestratorControl { policy, .. } => policy.as_ref(),
    }
}

/// Merge tool set defaults with per-tool policy.
fn merge_policy(
    tool_set_policy: Option<&ToolSetPolicy>,
    tool_policy: Option<&ToolPolicy>,
) -> Option<ToolPolicy> {
    match (tool_set_policy, tool_policy) {
        (None, None) => None,
        (Some(tsp), None) => tsp.defaults.clone(),
        (None, Some(tp)) => Some(tp.clone()),
        (Some(tsp), Some(tp)) => {
            let mut merged = tsp.defaults.clone().unwrap_or_default();
            if tp.sensitivity.is_some() {
                merged.sensitivity = tp.sensitivity.clone();
            }
            if tp.approval.is_some() {
                merged.approval = tp.approval.clone();
            }
            if tp.timeout_ms.is_some() {
                merged.timeout_ms = tp.timeout_ms;
            }
            if tp.execution_mode.is_some() {
                merged.execution_mode = tp.execution_mode.clone();
            }
            if tp.failure_mode.is_some() {
                merged.failure_mode = tp.failure_mode.clone();
            }
            Some(merged)
        }
    }
}

/// Map gateway-level ToolApprovalDecision to host-visible ApprovalDecision.
fn map_approval_decision(decision: &ToolApprovalDecision) -> piko_protocol::ApprovalDecision {
    use piko_protocol::ApprovalDecision;
    match decision {
        ToolApprovalDecision::Accept => ApprovalDecision::Accept,
        ToolApprovalDecision::Decline => ApprovalDecision::Decline,
        ToolApprovalDecision::AcceptSession => ApprovalDecision::AcceptSession,
        ToolApprovalDecision::AcceptWorkspace => ApprovalDecision::AcceptWorkspace,
        ToolApprovalDecision::AcceptPermanent => ApprovalDecision::AcceptSession,
    }
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_project_tool_def_never_approval() {
        let tool = ToolDef {
            name: "test_tool".into(),
            description: "".into(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: "native".into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        };

        let policy = ToolPolicy {
            approval: Some(ToolApprovalPolicy::Never),
            ..Default::default()
        };

        let projected = project_tool_def(&tool, "test_tool", Some(&policy));
        assert_eq!(projected.approval, Some(ToolApprovalRequirement::Never));
    }

    #[tokio::test]
    async fn test_project_tool_def_dangerous_sensitivity() {
        let tool = ToolDef {
            name: "dangerous_tool".into(),
            description: "".into(),
            input_schema: serde_json::json!({}),
            executor: crate::domain::tools::definition::ToolExecutorRef {
                kind: "native".into(),
                target: "test".into(),
                extra: None,
            },
            execution_mode: None,
            exposure: None,
            capabilities: None,
            approval: None,
            metadata: None,
        };

        let policy = ToolPolicy {
            sensitivity: Some(ToolSensitivity::Dangerous),
            ..Default::default()
        };

        let projected = project_tool_def(&tool, "dangerous_tool", Some(&policy));
        assert_eq!(projected.approval, Some(ToolApprovalRequirement::Always));
    }
}
