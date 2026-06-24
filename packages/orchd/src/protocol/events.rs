// ---- Protocol: events — host-visible streaming events ----

use serde::{Deserialize, Serialize};

use super::messages::Usage;

// ================================================================
// OrchEvent — orchd-native public event stream (replaces HostEvent
// for the public OrchRuntime API)
// ================================================================

/// orchd-native event emitted to Host via subscribe().
///
/// Simpler and more focused than the pi-compatible HostEvent below.
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

use super::agents::AgentTaskResult;
use super::messages::Message;
use super::runtime_stream::{RuntimeAssistantMessageEvent, RuntimeMessage, RuntimeToolOrder};

// ---- HostOrderBase ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostOrderBase {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_index: Option<u32>,
}

// ---- HostEvent enum ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HostEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "message_update")]
    MessageUpdate {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
        #[serde(skip_serializing_if = "Option::is_none", rename = "assistantEvent")]
        assistant_event: Option<RuntimeAssistantMessageEvent>,
    },
    #[serde(rename = "message_end")]
    MessageEnd {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "token")]
    Token {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        text: String,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        text: String,
    },
    #[serde(rename = "tool_start")]
    ToolStart {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        id: String,
        name: String,
        args: serde_json::Value,
    },
    #[serde(rename = "tool_end")]
    ToolEnd {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        id: String,
        name: String,
        result: serde_json::Value,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    #[serde(rename = "approval_needed")]
    ApprovalNeeded {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "approvalId")]
        approval_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "toolArgs")]
        tool_args: serde_json::Value,
    },
    #[serde(rename = "approval_resolved")]
    ApprovalResolved {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "approvalId")]
        approval_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        decision: String, // "accept" or "decline"
    },
    #[serde(rename = "task_started")]
    TaskStarted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    #[serde(rename = "task_created")]
    TaskCreated { task: AgentTaskCreated },
    #[serde(rename = "task_completed")]
    TaskCompleted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        result: AgentTaskResult,
    },
    #[serde(rename = "task_transcript_committed")]
    TaskTranscriptCommitted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        messages: Vec<Message>,
        summary: String,
        #[serde(rename = "finalStatus")]
        final_status: String,
    },
    #[serde(rename = "task_failed")]
    TaskFailed {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        error: String,
    },
    #[serde(rename = "plan_updated")]
    PlanUpdated {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        plan: Vec<serde_json::Value>,
    },
    #[serde(rename = "done")]
    Done { status: String },
}

// ---- Helper type for TaskCreated payload ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskCreated {
    pub id: String,
    #[serde(rename = "targetAgentId")]
    pub target_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<super::agents::TaskSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "parentTaskId")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
}

// ---- HostEventListener ----

pub type HostEventCallback = Box<dyn Fn(&HostEvent) + Send + Sync>;

// ================================================================
// OrchestratorEvent — internal orchestrator events (AgentActor emits these)
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OrchestratorEvent {
    #[serde(rename = "task_delta")]
    TaskDelta {
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        delta: serde_json::Value,
    },
    #[serde(rename = "task_message_start")]
    TaskMessageStart {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "task_message_update")]
    TaskMessageUpdate {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
        #[serde(skip_serializing_if = "Option::is_none", rename = "assistantEvent")]
        assistant_event: Option<RuntimeAssistantMessageEvent>,
    },
    #[serde(rename = "task_message_end")]
    TaskMessageEnd {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        message: RuntimeMessage,
    },
    #[serde(rename = "task_started")]
    TaskStarted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
    },
    #[serde(rename = "task_completed")]
    TaskCompleted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        result: AgentTaskResult,
    },
    #[serde(rename = "task_transcript_committed")]
    TaskTranscriptCommitted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        messages: Vec<Message>,
        summary: String,
        #[serde(rename = "finalStatus")]
        final_status: String,
    },
    #[serde(rename = "task_failed")]
    TaskFailed {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        error: String,
    },
    #[serde(rename = "task_cancelled")]
    TaskCancelled {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    #[serde(rename = "plan_updated")]
    PlanUpdated {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        plan: serde_json::Value,
    },
    #[serde(rename = "tool_started")]
    ToolStarted {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "callId")]
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    #[serde(rename = "tool_finished")]
    ToolFinished {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(flatten)]
        tool_order: RuntimeToolOrder,
        #[serde(rename = "entityId")]
        entity_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "callId")]
        call_id: String,
        result: serde_json::Value,
    },
    #[serde(rename = "approval_requested")]
    ApprovalRequested {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "approvalId")]
        approval_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(rename = "toolArgs")]
        tool_args: serde_json::Value,
    },
    #[serde(rename = "approval_resolved")]
    ApprovalResolved {
        #[serde(flatten)]
        order: HostOrderBase,
        #[serde(rename = "toolEntityId")]
        tool_entity_id: String,
        #[serde(rename = "approvalId")]
        approval_id: String,
        #[serde(rename = "agentId")]
        agent_id: String,
        #[serde(rename = "taskId")]
        task_id: String,
        decision: String,
    },
}

impl TryFrom<HostEvent> for OrchEvent {
    type Error = &'static str;

    fn try_from(host_event: HostEvent) -> Result<Self, Self::Error> {
        match host_event {
            HostEvent::MessageStart {
                message,
                agent_id,
                task_id,
                ..
            } => {
                let message_id = match &message {
                    RuntimeMessage::User { id, .. } => id.clone(),
                    RuntimeMessage::Assistant { id, .. } => id.clone(),
                    RuntimeMessage::ToolResult { id, .. } => id.clone(),
                    RuntimeMessage::Custom { id, .. } => id.clone(),
                };
                Ok(OrchEvent::MessageStart {
                    message_id,
                    agent_id,
                    task_id,
                })
            }
            HostEvent::MessageUpdate {
                message,
                assistant_event,
                ..
            } => {
                let message_id = match &message {
                    RuntimeMessage::User { id, .. } => id.clone(),
                    RuntimeMessage::Assistant { id, .. } => id.clone(),
                    RuntimeMessage::ToolResult { id, .. } => id.clone(),
                    RuntimeMessage::Custom { id, .. } => id.clone(),
                };
                if let Some(ae) = assistant_event {
                    match ae {
                        RuntimeAssistantMessageEvent::TextDelta { delta, .. } => {
                            Ok(OrchEvent::TextDelta { message_id, delta })
                        }
                        RuntimeAssistantMessageEvent::ThinkingDelta { delta, .. } => {
                            Ok(OrchEvent::ThinkingDelta { message_id, delta })
                        }
                        _ => Err("Unsupported assistant message event variant"),
                    }
                } else {
                    Err("No assistant event delta in message update")
                }
            }
            HostEvent::MessageEnd { message, .. } => {
                let (message_id, stop_reason) = match &message {
                    RuntimeMessage::Assistant {
                        id, stop_reason, ..
                    } => (
                        id.clone(),
                        stop_reason.clone().unwrap_or_else(|| "stop".to_string()),
                    ),
                    m => (
                        match m {
                            RuntimeMessage::User { id, .. } => id.clone(),
                            RuntimeMessage::Assistant { id, .. } => id.clone(),
                            RuntimeMessage::ToolResult { id, .. } => id.clone(),
                            RuntimeMessage::Custom { id, .. } => id.clone(),
                        },
                        "stop".to_string(),
                    ),
                };
                Ok(OrchEvent::MessageEnd {
                    message_id,
                    stop_reason,
                })
            }
            HostEvent::ToolStart {
                id,
                name,
                agent_id,
                task_id,
                ..
            } => Ok(OrchEvent::ToolStart {
                tool_call_id: id,
                tool_name: name,
                agent_id,
                task_id,
            }),
            HostEvent::ToolEnd {
                id,
                result,
                is_error,
                ..
            } => Ok(OrchEvent::ToolEnd {
                tool_call_id: id,
                ok: !is_error,
                output: result,
            }),
            HostEvent::ApprovalNeeded {
                tool_name,
                tool_args,
                agent_id,
                task_id,
                ..
            } => Ok(OrchEvent::RequestApproval {
                action: tool_name,
                details: Some(tool_args.to_string()),
                agent_id,
                task_id,
            }),
            HostEvent::PlanUpdated {
                agent_id,
                task_id,
                plan,
                ..
            } => Ok(OrchEvent::PlanUpdated {
                agent_id,
                task_id,
                plan,
            }),
            HostEvent::TaskFailed { task_id, error, .. } => {
                Ok(OrchEvent::TaskError { task_id, error })
            }
            HostEvent::TaskCompleted { task_id, .. } => Ok(OrchEvent::TaskEnd {
                task_id,
                status: TaskEndStatus::Completed,
                usage: None,
            }),
            _ => Err("Unhandled host event variant"),
        }
    }
}
