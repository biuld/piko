//! Authoritative client-side projection partitions.

use std::collections::HashMap;

use piko_protocol::session::SessionTreeEntry;
use piko_protocol::{
    AgentInfo, AgentInstanceId, ApprovalId, CommandId, InteractionId, SessionId, SessionSnapshot,
    TurnId,
};

use crate::branch::active_branch_entries;
use crate::timeline::AgentTimeline;

/// Session phase machine. Identity results never enter [`SessionPhase::Live`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionPhase {
    #[default]
    IdleNoSession,
    OpeningOrCreating {
        target_id: Option<SessionId>,
    },
    Hydrating {
        target_id: SessionId,
    },
    Live,
}

/// What product operation a pending command correlates to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingOp {
    Discover,
    Open { session_id: SessionId },
    Create,
    Refresh,
    Delete { session_id: SessionId },
    Navigate { session_id: SessionId },
    Submit,
    Cancel,
    ApprovalRespond { approval_id: ApprovalId },
    InteractionRespond { interaction_id: InteractionId },
    SelectAgent { agent_instance_id: AgentInstanceId },
    ListModels,
    SetModel { provider: String, model_id: String },
    SetThinkingLevel { level: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandFailure {
    pub command_id: CommandId,
    pub operation: PendingOp,
    pub message: String,
}

/// Connection observation state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connected,
}

/// Shell-level state (transport, bootstrap).
#[derive(Debug, Clone, Default)]
pub struct ShellState {
    pub connection: ConnectionState,
}

/// A pending approval awaiting resolution.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    pub approval_id: ApprovalId,
    pub agent_instance_id: AgentInstanceId,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub response_in_flight: bool,
}

/// A pending interaction awaiting resolution.
#[derive(Debug, Clone)]
pub struct PendingInteraction {
    pub interaction_id: InteractionId,
    pub agent_instance_id: AgentInstanceId,
    pub questions: Vec<piko_protocol::InteractionQuestion>,
    pub require_confirm: bool,
    pub response_in_flight: bool,
}

/// Active turn tracking per agent.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveTurn {
    pub turn_id: TurnId,
    pub agent_instance_id: AgentInstanceId,
    pub status: piko_protocol::TurnStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnFailure {
    pub turn_id: TurnId,
    pub agent_instance_id: AgentInstanceId,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueProjection {
    pub steer_count: u32,
    pub follow_up_count: u32,
    pub next_turn_count: u32,
    pub steer_preview: Option<String>,
    pub follow_up_preview: Option<String>,
}

/// Session list projection.
#[derive(Debug, Clone, Default)]
pub struct SessionListProjection {
    pub sessions: Vec<piko_protocol::SessionSummary>,
}

/// Live session state: all authoritative projections from hostd.
#[derive(Debug, Clone, Default)]
pub struct LiveSession {
    pub session_id: SessionId,
    pub cwd: String,
    pub name: Option<String>,
    pub entries: Vec<piko_protocol::session::SessionTreeEntry>,
    pub current_leaf_id: Option<String>,
    pub agents: Vec<AgentInfo>,
    pub selected_agent: Option<AgentInstanceId>,
    pub timelines: HashMap<AgentInstanceId, AgentTimeline>,
    pub active_turns: Vec<ActiveTurn>,
    pub turn_failures: Vec<TurnFailure>,
    pub queue: QueueProjection,
    pub pending_approvals: Vec<PendingApproval>,
    pub pending_interactions: Vec<PendingInteraction>,
    pub cumulative_usage: Option<piko_protocol::messages::Usage>,
}

/// Model/thinking projection.
#[derive(Debug, Clone, Default)]
pub struct ModelState {
    pub model_id: Option<String>,
    pub provider: Option<String>,
    pub thinking_level: Option<String>,
    /// Catalog from the latest successful `ModelList`.
    pub providers: Vec<piko_protocol::ProviderInfo>,
}

/// The previous live session id kept for failed open/create recovery.
#[derive(Debug, Clone, Default)]
pub struct ClientState {
    pub shell: ShellState,
    pub session_phase: SessionPhase,
    pub live_session: Option<LiveSession>,
    pub session_list: SessionListProjection,
    pub model: ModelState,
    pub pending_commands: HashMap<CommandId, PendingOp>,
    /// Bounded correlated failures for frontends that recover operation-local UI.
    pub command_failures: Vec<CommandFailure>,
    pub last_error: Option<String>,
    /// Saved previous session id for recovery on failed open/create.
    previous_live_session_id: Option<SessionId>,
}

impl ClientState {
    pub fn is_live(&self) -> bool {
        self.session_phase == SessionPhase::Live
    }

    pub fn live_session_id(&self) -> Option<&str> {
        self.live_session.as_ref().map(|s| s.session_id.as_str())
    }

    pub(crate) fn save_previous_live(&mut self) {
        self.previous_live_session_id = self.live_session.as_ref().map(|s| s.session_id.clone());
    }

    pub(crate) fn take_previous_live(&mut self) -> Option<SessionId> {
        self.previous_live_session_id.take()
    }
}

impl LiveSession {
    pub fn from_reconcile(
        session_id: SessionId,
        snapshot: &SessionSnapshot,
        agents: &[AgentInfo],
    ) -> Self {
        let selected_agent =
            resolve_selected_agent(snapshot.selected_agent_instance_id.as_deref(), agents);

        let active_turns = snapshot
            .active_turns
            .iter()
            .map(|t| ActiveTurn {
                turn_id: t.turn_id.clone(),
                agent_instance_id: t.agent_instance_id.clone(),
                status: t.status.clone(),
            })
            .collect();

        let pending_approvals = snapshot
            .pending_approvals
            .iter()
            .map(|a| PendingApproval {
                approval_id: a.approval_id.clone(),
                agent_instance_id: a.agent_instance_id.clone(),
                tool_name: a.tool_name.clone(),
                tool_args: a.request.clone(),
                response_in_flight: false,
            })
            .collect();

        let pending_interactions = snapshot
            .pending_interactions
            .iter()
            .map(|i| PendingInteraction {
                interaction_id: i.interaction_id.clone(),
                agent_instance_id: i.agent_instance_id.clone(),
                questions: i.questions.clone(),
                require_confirm: i.require_confirm,
                response_in_flight: false,
            })
            .collect();

        let timelines =
            timelines_from_snapshot(&snapshot.entries, snapshot.current_leaf_id.as_deref());

        Self {
            session_id,
            cwd: snapshot.cwd.clone(),
            name: snapshot.name.clone(),
            entries: snapshot.entries.clone(),
            current_leaf_id: snapshot.current_leaf_id.clone(),
            agents: agents.to_vec(),
            selected_agent,
            timelines,
            active_turns,
            turn_failures: Vec::new(),
            queue: QueueProjection::default(),
            pending_approvals,
            pending_interactions,
            cumulative_usage: snapshot.cumulative_usage.clone(),
        }
    }
}

/// Build per-agent committed timelines from the active branch of the session tree.
fn timelines_from_snapshot(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> HashMap<AgentInstanceId, AgentTimeline> {
    let mut timelines: HashMap<AgentInstanceId, AgentTimeline> = HashMap::new();
    for entry in active_branch_entries(entries, current_leaf_id) {
        match entry {
            SessionTreeEntry::Message(message_entry) => {
                let timeline = timelines
                    .entry(message_entry.agent_instance_id.clone())
                    .or_default();
                timeline.apply_committed(
                    message_entry.id,
                    message_entry.transcript_seq,
                    message_entry.message,
                    message_entry.source_turn_id,
                );
            }
            SessionTreeEntry::ToolCall(tool) => {
                if let Some(agent_instance_id) = tool.agent_instance_id {
                    timelines
                        .entry(agent_instance_id)
                        .or_default()
                        .apply_tool_started(
                            tool.tool_call_id,
                            tool.tool_name,
                            tool.arguments,
                            tool.parent_message_id,
                        );
                }
            }
            _ => {}
        }
    }
    timelines
}

/// Resolve selected agent: prefer snapshot, fallback to root, else first.
fn resolve_selected_agent(selected: Option<&str>, agents: &[AgentInfo]) -> Option<AgentInstanceId> {
    if let Some(sel) = selected
        && agents.iter().any(|a| a.agent_instance_id == sel)
    {
        return Some(sel.to_string());
    }
    // Fallback: root agent (no parent)
    if let Some(root) = agents.iter().find(|a| a.parent_agent_instance_id.is_none()) {
        return Some(root.agent_instance_id.clone());
    }
    // Last resort: first agent
    agents.first().map(|a| a.agent_instance_id.clone())
}
