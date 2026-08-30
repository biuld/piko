use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRunEvent {
    Started {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Completed {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        timestamp: i64,
    },
    Failed {
        session_id: SessionId,
        run_id: String,
        agent_instance_id: crate::AgentInstanceId,
        error: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalEvent {
    Requested {
        session_id: SessionId,
        agent_instance_id: crate::AgentInstanceId,
        agent_id: AgentId,
        approval_id: ApprovalId,
        tool_name: String,
        tool_args: serde_json::Value,
        /// F-13: operator-authored approval prompt (MCP approval templates);
        /// absent → clients keep the generic question.
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    Resolved {
        session_id: SessionId,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
}

impl From<ApprovalEvent> for ServerMessage {
    fn from(event: ApprovalEvent) -> Self {
        Self::Approval(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueueEvent {
    Updated {
        session_id: SessionId,
        steer_count: u32,
        follow_up_count: u32,
        next_turn_count: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        steer_preview: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        follow_up_preview: Option<String>,
    },
}

impl From<QueueEvent> for ServerMessage {
    fn from(event: QueueEvent) -> Self {
        Self::Queue(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelEvent {
    ConfigChanged {
        model_id: String,
        provider: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingLevel")]
        thinking_level: Option<crate::model::ThinkingLevel>,
        /// Active model context window (tokens) from the host catalog when known.
        /// Clients use this for status chrome (`used/size`) without relying on a
        /// local catalog cache (F-22 / D-34).
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            rename = "contextWindow"
        )]
        context_window: Option<u64>,
        timestamp: i64,
    },
}

impl From<ModelEvent> for ServerMessage {
    fn from(event: ModelEvent) -> Self {
        Self::Model(event)
    }
}

/// Live usage / context chrome projection (ACP-inspired `usage_update`, piko-native).
///
/// Prefer this for status bars over client-side roll-up of turn usage alone.
/// Clients may treat `cumulative` as the authoritative session ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UsageEvent {
    Updated {
        session_id: SessionId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_instance_id: Option<crate::AgentInstanceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<TurnId>,
        /// Context fill estimate (prompt side: `input + cache_read`).
        used: u64,
        /// Active model context window when host can resolve it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<u64>,
        /// Session cumulative ledger after this update (when known).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cumulative: Option<crate::messages::Usage>,
        /// Turn-scoped usage that triggered this projection (when applicable).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_usage: Option<crate::messages::Usage>,
        timestamp: i64,
    },
}

impl From<UsageEvent> for ServerMessage {
    fn from(event: UsageEvent) -> Self {
        Self::Usage(event)
    }
}

impl From<crate::StreamItemPatch> for ServerMessage {
    fn from(event: crate::StreamItemPatch) -> Self {
        Self::StreamItem(event)
    }
}
