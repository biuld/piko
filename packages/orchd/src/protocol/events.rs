// ---- Protocol: events — orchd-native event stream ----

use serde::{Deserialize, Serialize};

use super::messages::Usage;

/// orchd-native event emitted to Host via subscribe_orch().
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrchEvent {
    // ── Streaming output ──
    /// LLM begins a new assistant message.
    MessageStart {
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },

    /// Text delta (typewriter effect).
    TextDelta {
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },

    /// Thinking / reasoning delta.
    ThinkingDelta {
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },

    /// LLM finishes an assistant message.
    MessageEnd {
        #[serde(rename = "messageId")]
        message_id: String,
        #[serde(rename = "stopReason")]
        stop_reason: String,
    },

    // ── Tool execution ──
    /// A tool call is about to be executed.
    ToolStart {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },

    /// A tool call completed.
    ToolEnd {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        ok: bool,
        output: serde_json::Value,
    },

    // ── User interaction (Host must respond via respond_user) ──
    /// Agent needs to ask the user a question.
    AskUser {
        question: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },

    /// Agent needs user approval for an action.
    RequestApproval {
        #[serde(rename = "approvalId")]
        approval_id: String,
        action: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },

    // ── Sub-agent lifecycle ──
    /// A sub-agent task was spawned.
    SubAgentSpawned {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        mode: SpawnMode,
    },

    /// A sub-agent task completed.
    SubAgentCompleted {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        result: serde_json::Value,
    },

    // ── State changes ──
    /// The agent's plan was updated.
    PlanUpdated {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        plan: Vec<serde_json::Value>,
    },

    // ── Lifecycle ──
    /// A task-level error occurred.
    TaskError {
        #[serde(rename = "taskId")]
        task_id: String,
        error: String,
    },

    /// A task completed or was aborted.
    TaskEnd {
        #[serde(rename = "taskId")]
        task_id: String,
        status: TaskEndStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpawnMode {
    Call,
    Detach,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskEndStatus {
    Completed,
    Aborted,
    Error,
}
