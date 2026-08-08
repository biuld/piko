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

mod impls;
mod trait_impl;
