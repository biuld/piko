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
use crate::domain::tools::call::ToolCall;
use crate::domain::tools::definition::{
    ToolApprovalRequirement, ToolDef, ToolExecutionMode, ToolSet, ToolSetToolRef,
};
use crate::domain::tools::result::{ToolExecError, ToolExecResult};
use crate::ports::approval_gateway::ApprovalGateway;
use crate::ports::tool_provider::{ToolDiscoveryContext, ToolExecutionContext, ToolProvider};
use crate::runtime::utils::runtime_tool_entity_id;

use super::catalog::{CatalogEntry, add_entry, merge_policy, tool_ref_policy};
use super::features::{disabled_feature_for_tool_name, feature_enabled};

// ---- CatalogRoute ----

/// Route from public tool name to the provider that implements it.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogRoute {
    pub provider_id: String,
    pub provider_tool_name: String,
    pub tool_def: ToolDef,
    /// Effective execution mode resolved at catalog build time (per-tool
    /// override, then set-level `allowParallel`, then fail-closed sequential).
    pub execution_mode: ToolExecutionMode,
    /// Concurrency cap inherited from the owning tool set policy; the runtime
    /// enforces it per batch without re-reading tool sets.
    pub max_concurrent_calls: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionRecord {
    pub result: ToolExecResult,
}

// ---- ToolRegistry trait ----

/// Public interface for tool discovery and execution.
#[async_trait]
pub trait ToolRegistry: Send + Sync {
    /// Discover tools available for the given context.
    async fn discover_tools(
        &self,
        context: &ToolDiscoveryContext,
    ) -> Result<(Vec<ToolDef>, HashMap<String, CatalogRoute>), String>;

    /// Execute a tool call through its registered provider.
    ///
    /// `call` should be `ToolCall` struct — other types will
    /// produce an immediate error result.
    async fn execute_tool(
        &self,
        call: &ToolCall,
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
    /// Resolved managed-feature set (F-18). `None` = all features enabled.
    features: RwLock<Option<HashMap<String, bool>>>,
}

impl Default for ToolRegistryImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistryImpl {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
            tool_sets: RwLock::new(HashMap::new()),
            approval_gateway: RwLock::new(None),
            features: RwLock::new(None),
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

    /// Install the resolved managed-feature set (F-18). `None` keeps every
    /// feature enabled (today's behavior).
    pub async fn set_features(&self, features: Option<HashMap<String, bool>>) {
        *self.features.write().await = features;
    }

    /// Return the feature key that disables `tool_name`, if any. Used by the
    /// direct-call error path when a tool has no route.
    pub async fn feature_gate(&self, tool_name: &str) -> Option<String> {
        let features = self.features.read().await;
        disabled_feature_for_tool_name(features.as_ref(), tool_name).map(str::to_string)
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
                        agent_instance_id: ctx.agent_instance_id.clone(),
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
                                tool_set.policy.as_ref(),
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
                                tool_set.policy.as_ref(),
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
                                    tool_set.policy.as_ref(),
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
    ) -> Result<(Vec<ToolDef>, HashMap<String, CatalogRoute>), String> {
        let catalog = self.build_catalog(context).await?;

        let features = self.features.read().await;

        // Apply feature gating (F-18) then active tool name restrictions.
        // A tool passes when its feature is enabled (or ungated) and, when a
        // transient allow-list is present, its name is listed.
        let tools: Vec<ToolDef> = if let Some(ref active) = context.active_tool_names {
            catalog
                .iter()
                .filter(|e| {
                    feature_enabled(features.as_ref(), &e.tool_def)
                        && active.contains(&e.public_name)
                })
                .map(|e| e.tool_def.clone())
                .collect()
        } else {
            catalog
                .iter()
                .filter(|e| feature_enabled(features.as_ref(), &e.tool_def))
                .map(|e| e.tool_def.clone())
                .collect()
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
            if !feature_enabled(features.as_ref(), &entry.tool_def) {
                continue;
            }
            routes.insert(
                entry.public_name.clone(),
                CatalogRoute {
                    provider_id: entry.provider_id.clone(),
                    provider_tool_name: entry.provider_tool_name.clone(),
                    tool_def: entry.tool_def.clone(),
                    execution_mode: entry.execution_mode.clone(),
                    max_concurrent_calls: entry.max_concurrent_calls,
                },
            );
        }

        Ok((tools, routes))
    }

    /// Execute a tool call with approval checks.
    async fn execute_tool(
        &self,
        call: &ToolCall,
        context: &ToolExecutionContext,
        route: &CatalogRoute,
        cancel: Option<CancellationToken>,
    ) -> ToolExecutionRecord {
        let call_id = call.id.clone();
        let call_name = call.name.clone();
        let call_args = call.arguments.clone();

        // Compute ordering metadata
        let tool_entity_id = context.tool_entity_id.clone().unwrap_or_else(|| {
            runtime_tool_entity_id(
                context.parent_message_id.as_deref().unwrap_or(""),
                context.tool_call_index.unwrap_or(0),
            )
        });

        // ---- Check cancellation ----
        if let Some(ref token) = cancel
            && token.is_cancelled()
        {
            return ToolExecutionRecord {
                result: ToolExecResult {
                    ok: false,
                    value: None,
                    error: Some(ToolExecError {
                        code: "aborted".into(),
                        message: "Task cancelled".into(),
                        retryable: Some(false),
                    }),
                },
            };
        }

        // ---- Look up provider ----
        let providers = self.providers.read().await;
        let provider = match providers.get(&route.provider_id) {
            Some(p) => p,
            None => {
                return ToolExecutionRecord {
                    result: ToolExecResult {
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
                    },
                };
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
                    return ToolExecutionRecord {
                        result: ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "aborted".into(),
                                message: "Task cancelled".into(),
                                retryable: Some(false),
                            }),
                        },
                    };
                }

                let gateway = self.approval_gateway.read().await;
                if let Some(gw) = gateway.as_ref() {
                    // Race approval against cancellation
                    // F-12 safety evidence: the provider projects its
                    // enforceable writable roots so hostd can assess write
                    // targets deterministically before any user/guardian flow.
                    // F-19: the provider projects the enforcing role's
                    // writable roots so hostd's safety assessment matches
                    // the policy the call will actually run under.
                    let writable_roots = provider.writable_roots_for(context).map(|roots| {
                        roots
                            .iter()
                            .map(|root| root.display().to_string())
                            .collect()
                    });
                    let approval_request = ToolApprovalRequest {
                        tool_entity_id: tool_entity_id.clone(),
                        call_id: call_id.clone(),
                        agent_id: context.agent_id.clone(),
                        agent_instance_id: context.agent_instance_id.clone(),
                        agent_role: context.agent_role.clone(),
                        tool_name: call_name.clone(),
                        tool_args: call_args.clone(),
                        host_context: context.host_context.clone(),
                        writable_roots,
                    };

                    let decision = if let Some(token) = cancel {
                        tokio::select! {
                            d = gw.request_tool_approval(approval_request) => d,
                            _ = token.cancelled() => ToolApprovalDecision::Decline,
                        }
                    } else {
                        gw.request_tool_approval(approval_request).await
                    };

                    if !piko_orchd_api::is_approval_accepted(&decision) {
                        let (code, message): (&str, String) = match decision {
                            ToolApprovalDecision::Expired => (
                                "approval_expired",
                                "Approval request expired before a decision arrived".into(),
                            ),
                            ToolApprovalDecision::GuardianDenied { reason } => (
                                "guardian_denied",
                                format!("Guardian denied approval: {reason}"),
                            ),
                            ToolApprovalDecision::GuardianUnavailable => (
                                "guardian_unavailable",
                                "Guardian review failed; failing closed".into(),
                            ),
                            ToolApprovalDecision::SafetyRejected { reason } => (
                                "safety_rejected",
                                format!("Write rejected by safety assessment: {reason}"),
                            ),
                            ToolApprovalDecision::PermissionDenied { reason } => (
                                "permission_denied",
                                format!("Command denied by permission policy: {reason}"),
                            ),
                            _ => ("declined", "User declined approval".into()),
                        };
                        return ToolExecutionRecord {
                            result: ToolExecResult {
                                ok: false,
                                value: None,
                                error: Some(ToolExecError {
                                    code: code.into(),
                                    message,
                                    retryable: Some(false),
                                }),
                            },
                        };
                    }
                } else {
                    // No approval gateway configured — deny tools that need approval.
                    return ToolExecutionRecord {
                        result: ToolExecResult {
                            ok: false,
                            value: None,
                            error: Some(ToolExecError {
                                code: "approval_unavailable".into(),
                                message: format!(
                                    "Tool '{call_name}' requires approval but no ApprovalGateway is configured"
                                ),
                                retryable: Some(false),
                            }),
                        },
                    };
                }
            }
        }

        // ---- Execute provider ----
        let provider_call = if route.provider_tool_name != call_name {
            ToolCall {
                id: call_id.clone(),
                name: route.provider_tool_name.clone(),
                arguments: call_args.clone(),
                partial_json: None,
            }
        } else {
            call.clone()
        };

        let exec_context = ToolExecutionContext {
            session_id: context.session_id.clone(),
            agent_instance_id: context.agent_instance_id.clone(),
            execution_id: context.execution_id.clone(),
            cancellation: context.cancellation.clone(),
            agent_id: context.agent_id.clone(),
            agent_role: context.agent_role.clone(),
            tool_set_ids: context.tool_set_ids.clone(),
            turn_index: context.turn_index,
            event_seq: context.event_seq,
            next_event_seq: context.next_event_seq,
            parent_message_id: context.parent_message_id.clone(),
            content_index: context.content_index,
            tool_call_index: context.tool_call_index,
            tool_entity_id: Some(tool_entity_id.clone()),
            host_context: context.host_context.clone(),
            source_turn_id: context.source_turn_id.clone(),
            context_remaining: context.context_remaining,
        };

        let exec_result = provider.execute(provider_call, exec_context).await;

        ToolExecutionRecord {
            result: exec_result,
        }
    }
}
