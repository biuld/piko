//! Stream item identity and patch semantics (F-22 / D-34).
//!
//! Sole host→client stream envelope (`ServerMessage::StreamItem`).
//!
//! ## Kind coverage (Slice 3)
//!
//! | Kind | Live host path |
//! |---|---|
//! | `UserMessage` | Reserved on StreamItem; durable user text uses `TranscriptCommitted` |
//! | `AgentMessage` / `AgentThought` / `ToolCall` | Emitted (realtime + tool mapping) |
//! | `Plan` | **Deferred** — reserved; no host emitter until plan UX ships (F-22) |
//! | `Usage` | **Not used on StreamItem** — live usage is `ServerMessage::Usage` |
//! | `System` | **Deferred** — system/context markers stay on transcript/other events |

use serde::{Deserialize, Serialize};

use crate::agent_runtime::RealtimeDelta;
use crate::event::ToolExecutionEvent;
use crate::{AgentInstanceId, MessageRole, SessionId};

/// Logical stream item class for client projections.
///
/// `Plan` and `System` are reserved for a future product path (deferred in
/// F-22 / D-34); clients must tolerate unknown ops on those kinds as no-ops
/// until an emitter lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamItemKind {
    UserMessage,
    AgentMessage,
    AgentThought,
    ToolCall,
    /// Provider-side ("upstream") tool lifecycle live on the stream.
    Upstream,
    /// Deferred: no host→client emitter yet (plan UX not productized).
    Plan,
    /// Reserved; live usage uses [`crate::UsageEvent`] / `ServerMessage::Usage`.
    Usage,
    /// Deferred: system/context markers not yet folded into StreamItem.
    System,
}

/// How a patch mutates the item identified by `item_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamItemOp {
    Upsert,
    AppendChunk,
    /// Replace the addressed `content_index` without changing item identity.
    ReplaceContent,
    /// Clear the addressed segment, or all segments when no index is supplied.
    ClearContent,
}

/// Wire patch envelope for `ServerMessage::StreamItem`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StreamItemPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_instance_id: Option<AgentInstanceId>,
    /// Stable host identity: message id, tool call id, plan id, …
    pub item_id: String,
    /// Named `itemKind` so it does not collide with ServerMessage tag `kind`.
    pub item_kind: StreamItemKind,
    pub op: StreamItemOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Stable segment id scoped by `item_kind`; never a byte/chunk offset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    /// Realtime ordering key for message-scoped stream deltas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<serde_json::Value>,
}

impl StreamItemPatch {
    /// Map a realtime delta onto stream patches for one message id.
    pub fn from_realtime_delta(
        session_id: Option<SessionId>,
        agent_instance_id: Option<AgentInstanceId>,
        message_id: &str,
        delta_seq: Option<u64>,
        delta: &RealtimeDelta,
    ) -> Vec<Self> {
        match delta {
            RealtimeDelta::MessageStarted { role } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: message_id.to_string(),
                    item_kind: StreamItemKind::AgentMessage,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq,
                    fields: Some(serde_json::json!({
                        "phase": "started",
                        "role": role,
                        "parentMessageId": message_id,
                    })),
                }]
            }
            RealtimeDelta::Text {
                content_index,
                delta,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: message_id.to_string(),
                    item_kind: StreamItemKind::AgentMessage,
                    op: StreamItemOp::AppendChunk,
                    text: Some(delta.clone()),
                    content_index: Some(*content_index),
                    delta_seq,
                    fields: Some(serde_json::json!({ "parentMessageId": message_id })),
                }]
            }
            RealtimeDelta::Thinking {
                content_index,
                delta,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: message_id.to_string(),
                    item_kind: StreamItemKind::AgentThought,
                    op: StreamItemOp::AppendChunk,
                    text: Some(delta.clone()),
                    content_index: Some(*content_index),
                    delta_seq,
                    fields: Some(serde_json::json!({ "parentMessageId": message_id })),
                }]
            }
            RealtimeDelta::ToolCall {
                tool_call_id,
                delta,
                content_index,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: tool_call_id.clone(),
                    item_kind: StreamItemKind::ToolCall,
                    op: StreamItemOp::AppendChunk,
                    text: Some(delta.clone()),
                    content_index: Some(*content_index),
                    delta_seq,
                    fields: Some(serde_json::json!({
                        "parentMessageId": message_id,
                    })),
                }]
            }
            RealtimeDelta::UpstreamActivity {
                activity_id,
                tool_name,
                kind,
                status,
                arguments,
                action,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: activity_id.clone(),
                    item_kind: StreamItemKind::Upstream,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq,
                    fields: Some(serde_json::json!({
                        "status": upstream_status_str(*status),
                        "toolName": tool_name,
                        "kind": kind,
                        "args": arguments,
                        "action": action,
                        "parentMessageId": message_id,
                    })),
                }]
            }
            RealtimeDelta::UpstreamApproval {
                approval_id,
                tool_name,
                summary,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: approval_id.clone(),
                    item_kind: StreamItemKind::Upstream,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq,
                    fields: Some(serde_json::json!({
                        "status": "approval",
                        "toolName": tool_name,
                        "summary": summary,
                        "parentMessageId": message_id,
                    })),
                }]
            }
            RealtimeDelta::MessageEnded {
                stop_reason,
                error_message,
            } => {
                vec![Self {
                    session_id,
                    agent_instance_id,
                    item_id: message_id.to_string(),
                    item_kind: StreamItemKind::AgentMessage,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq,
                    fields: Some(serde_json::json!({
                        "phase": "ended",
                        "stopReason": stop_reason,
                        "errorMessage": error_message,
                        "parentMessageId": message_id,
                    })),
                }]
            }
        }
    }

    /// Map a tool execution lifecycle event to stream patches.
    pub fn from_tool_execution(event: &ToolExecutionEvent) -> Vec<Self> {
        match event {
            ToolExecutionEvent::Started {
                session_id,
                agent_instance_id,
                tool_call_id,
                tool_name,
                args,
                parent_message_id,
                root_input_id,
                ..
            } => {
                vec![Self {
                    session_id: Some(session_id.clone()),
                    agent_instance_id: Some(agent_instance_id.clone()),
                    item_id: tool_call_id.clone(),
                    item_kind: StreamItemKind::ToolCall,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq: None,
                    fields: Some(serde_json::json!({
                        "toolName": tool_name,
                        "args": args,
                        "status": "running",
                        "parentMessageId": parent_message_id,
                        "rootInputId": root_input_id,
                    })),
                }]
            }
            ToolExecutionEvent::Ended {
                session_id,
                agent_instance_id,
                tool_call_id,
                tool_name,
                result,
                is_error,
                parent_message_id,
                root_input_id,
                ..
            } => {
                let status = if *is_error { "failed" } else { "completed" };
                vec![Self {
                    session_id: Some(session_id.clone()),
                    agent_instance_id: Some(agent_instance_id.clone()),
                    item_id: tool_call_id.clone(),
                    item_kind: StreamItemKind::ToolCall,
                    op: StreamItemOp::Upsert,
                    text: None,
                    content_index: None,
                    delta_seq: None,
                    fields: Some(serde_json::json!({
                        "toolName": tool_name,
                        "result": result,
                        "status": status,
                        "parentMessageId": parent_message_id,
                        "rootInputId": root_input_id,
                    })),
                }]
            }
        }
    }

    /// Reconstruct a realtime apply key for timeline draft sequencing.
    /// Returns `(message_id, delta_seq, delta)`.
    pub fn as_realtime_apply(&self) -> Option<(String, u64, RealtimeDelta)> {
        let seq = self.delta_seq?;
        let parent = self
            .fields
            .as_ref()
            .and_then(|f| f.get("parentMessageId"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| self.item_id.clone());

        let delta = match (self.item_kind, self.op) {
            (StreamItemKind::AgentMessage, StreamItemOp::Upsert) => {
                let phase = self
                    .fields
                    .as_ref()
                    .and_then(|f| f.get("phase"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("started");
                match phase {
                    "started" => {
                        let role = self
                            .fields
                            .as_ref()
                            .and_then(|f| f.get("role"))
                            .and_then(|v| serde_json::from_value::<MessageRole>(v.clone()).ok())
                            .unwrap_or(MessageRole::Assistant);
                        RealtimeDelta::MessageStarted { role }
                    }
                    "ended" => RealtimeDelta::MessageEnded {
                        stop_reason: self
                            .fields
                            .as_ref()
                            .and_then(|f| f.get("stopReason"))
                            .filter(|v| !v.is_null())
                            .and_then(|v| v.as_str().map(str::to_string)),
                        error_message: self
                            .fields
                            .as_ref()
                            .and_then(|f| f.get("errorMessage"))
                            .filter(|v| !v.is_null())
                            .and_then(|v| v.as_str().map(str::to_string)),
                    },
                    _ => return None,
                }
            }
            (StreamItemKind::AgentMessage, StreamItemOp::AppendChunk) => RealtimeDelta::Text {
                content_index: self.content_index.unwrap_or(0),
                delta: self.text.clone().unwrap_or_default(),
            },
            (StreamItemKind::AgentThought, StreamItemOp::AppendChunk) => RealtimeDelta::Thinking {
                content_index: self.content_index.unwrap_or(0),
                delta: self.text.clone().unwrap_or_default(),
            },
            (StreamItemKind::ToolCall, StreamItemOp::AppendChunk) => RealtimeDelta::ToolCall {
                content_index: self.content_index.unwrap_or(0),
                tool_call_id: self.item_id.clone(),
                delta: self.text.clone().unwrap_or_default(),
            },
            _ => return None,
        };
        Some((parent, seq, delta))
    }
}

fn upstream_status_str(status: crate::messages::UpstreamActivityStatus) -> &'static str {
    match status {
        crate::messages::UpstreamActivityStatus::Started
        | crate::messages::UpstreamActivityStatus::InProgress => "running",
        crate::messages::UpstreamActivityStatus::Completed => "completed",
        crate::messages::UpstreamActivityStatus::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_delta_maps_to_append_agent_message() {
        let patches = StreamItemPatch::from_realtime_delta(
            Some("session-1".into()),
            Some("agent-1".into()),
            "msg-1",
            Some(3),
            &RealtimeDelta::Text {
                content_index: 0,
                delta: "hi".into(),
            },
        );
        assert_eq!(patches[0].item_kind, StreamItemKind::AgentMessage);
        assert_eq!(patches[0].op, StreamItemOp::AppendChunk);
        assert_eq!(patches[0].delta_seq, Some(3));
        let (mid, seq, delta) = patches[0].as_realtime_apply().unwrap();
        assert_eq!(mid, "msg-1");
        assert_eq!(seq, 3);
        assert!(matches!(delta, RealtimeDelta::Text { delta, .. } if delta == "hi"));
    }

    #[test]
    fn tool_call_delta_uses_tool_call_id_as_item_id() {
        let patches = StreamItemPatch::from_realtime_delta(
            None,
            None,
            "msg-1",
            Some(2),
            &RealtimeDelta::ToolCall {
                content_index: 0,
                tool_call_id: "call-9".into(),
                delta: "{\"x\":".into(),
            },
        );
        assert_eq!(patches[0].item_id, "call-9");
        let (mid, _, delta) = patches[0].as_realtime_apply().unwrap();
        assert_eq!(mid, "msg-1");
        assert!(matches!(
            delta,
            RealtimeDelta::ToolCall { tool_call_id, .. } if tool_call_id == "call-9"
        ));
    }

    #[test]
    fn message_started_upserts_identity() {
        let patches = StreamItemPatch::from_realtime_delta(
            None,
            None,
            "msg-1",
            Some(1),
            &RealtimeDelta::MessageStarted {
                role: MessageRole::Assistant,
            },
        );
        assert_eq!(patches[0].op, StreamItemOp::Upsert);
        assert!(patches[0].as_realtime_apply().is_some());
    }
}
