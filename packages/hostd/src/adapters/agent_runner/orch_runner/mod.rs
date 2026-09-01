use std::collections::HashMap;
use std::sync::Arc;

use piko_orchd::AgentRuntime;

use crate::adapters::agent_runner::approval::ApprovalStore;
use crate::api::UserInteractionResponse;
use crate::domain::guardian::{GuardianConfig, GuardianReviewCallback, GuardianState};
use crate::domain::permissions::PermissionConfig;
use crate::domain::safety::SafetyConfig;

mod agent_commit;
mod agent_input;
mod approval_gateway;
mod attach;
mod bootstrap;
mod commit;
mod interactions;
mod observation_router;
mod prompt_assembly;
mod run;
mod runner;

#[cfg(test)]
mod tests;

use crate::infra::trajectory::TrajectoryRecorderRegistry;
use commit::{ExecutionCommitRouter, RealtimeDeltaRouter};

type AgentInputKey = (String, String);
type AgentHubMap = HashMap<AgentInputKey, Arc<piko_orchd::events::SessionOutputHub>>;

/// Live observation state for one admitted AgentInput. `input_id` is the
/// durable control identity (the root input id when applied as root), so no
/// separate run/execution id is retained.
pub(crate) struct ActiveAgentRunRuntime {
    pub(crate) agent_instance_id: String,
    pub(crate) observation: Arc<piko_orchd::events::SessionOutputHub>,
    /// Hub position captured before this input begins processing. A later
    /// subscription must replay reliable commits produced before it attaches.
    pub(crate) observation_cursor: piko_protocol::agent_runtime::SessionCursor,
    pub(crate) input_id: String,
}

#[derive(Clone)]
pub struct OrchAgentRunRunner {
    agent_runtime: Arc<AgentRuntime>,
    active_agent_inputs: Arc<std::sync::Mutex<HashMap<AgentInputKey, ActiveAgentRunRuntime>>>,
    agent_hubs: Arc<std::sync::Mutex<AgentHubMap>>,
    pub(crate) trajectory_recorders: TrajectoryRecorderRegistry,
    commit_routers: Arc<std::sync::Mutex<HashMap<String, Arc<ExecutionCommitRouter>>>>,
    realtime_routers: Arc<std::sync::Mutex<HashMap<String, Arc<RealtimeDeltaRouter>>>>,
    pending_approvals: Arc<std::sync::Mutex<HashMap<String, PendingApprovalEntry>>>,
    pending_interactions: Arc<std::sync::Mutex<HashMap<String, PendingInteractionEntry>>>,
    approval_stores: Arc<std::sync::Mutex<HashMap<String, Arc<ApprovalStore>>>>,
    approval_timeout: std::time::Duration,
    guardian_config: Option<GuardianConfig>,
    guardian_review: Arc<std::sync::RwLock<Option<GuardianReviewCallback>>>,
    guardian_states: Arc<std::sync::Mutex<HashMap<String, GuardianState>>>,
    safety_config: SafetyConfig,
    permission_config: PermissionConfig,
    /// F-13: `[mcp.approval-templates]` keyed by `"server/tool"` or `"tool"`.
    /// Resolved into `ApprovalSnapshot.prompt` for MCP tool approvals.
    mcp_approval_templates: HashMap<String, String>,
    /// F-13: configured MCP server names, used to scope approval-template
    /// resolution to MCP tools (bare `tool` keys never match non-MCP tools).
    mcp_server_names: std::collections::HashSet<String>,
    /// F-13: per-server connection status for the `mcp.status` client
    /// surface (connected + counts, or the connect error).
    mcp_server_statuses: Vec<piko_protocol::command::McpServerInfo>,
    /// F-19: role → command policy for the approval gateway. Absent roles
    /// use `permission_config` (the session profile).
    role_permission_configs: HashMap<String, PermissionConfig>,
    session_contexts: Arc<std::sync::Mutex<HashMap<String, String>>>,
    session_attach_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    observation_router: Arc<observation_router::SessionObservationRouter>,
    prompt_gate: Arc<tokio::sync::Mutex<()>>,
    context_tools: Arc<piko_orchd::tools::ContextToolsProvider>,
}

struct PendingApprovalEntry {
    session_id: Option<String>,
    snapshot: crate::api::ApprovalSnapshot,
    tx: piko_comms::ReplySender<piko_comms::contracts::ApprovalReply, crate::api::ApprovalDecision>,
}

struct PendingInteractionEntry {
    session_id: Option<String>,
    snapshot: crate::api::UserInteractionSnapshot,
    tx: piko_comms::ReplySender<piko_comms::contracts::InteractionReply, UserInteractionResponse>,
}
