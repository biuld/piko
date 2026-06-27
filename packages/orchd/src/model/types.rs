// ---- Model: internal types for ModelStepExecutor subsystem ----
//
// These are orchestrator-internal types, NOT the public protocol types.

use serde::{Deserialize, Serialize};

use crate::protocol::{
    messages::{Message, Usage},
    model::{ModelProviderConfig, ModelRunSettings},
    tools::ToolDef,
};

/// Produce a stable runtime assistant message ID.
pub fn runtime_assistant_message_id(run_id: &str, step_id: &str) -> String {
    format!("{run_id}:{step_id}:assistant")
}

// ---- Input / output ----

/// Input for a single model step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStepInput {
    pub run_id: String,
    pub step_id: String,
    pub transcript: Vec<Message>,
    pub system_prompt: String,
    pub model: ModelSpec,
    pub provider: ModelProviderConfig,
    pub settings: ModelRunSettings,
    pub tools: Vec<ToolDef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_state: Option<serde_json::Value>,
}

/// Lightweight model reference (not the full pi-ai Model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub provider: String,
}

/// Events emitted during a model step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelStepEvent {
    StepStart,
    StepEnd,
    MessageStart {
        message: crate::stream::RuntimeMessage,
    },
    MessageUpdate {
        message: crate::stream::RuntimeMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        assistant_event: Option<crate::stream::RuntimeAssistantMessageEvent>,
    },
    MessageEnd {
        message: crate::stream::RuntimeMessage,
    },
    MessageDelta {
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    ThinkingDelta {
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    ProviderToolCallDelta {
        id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none", rename = "argsDelta")]
        args_delta: Option<String>,
    },
    Error {
        message: String,
    },
}

/// Result of a single model step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStepResult {
    pub status: String,
    pub appended_messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transcript_delta: Vec<TranscriptDelta>,
    pub stop_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine_state: Option<serde_json::Value>,
}

/// Delta for a transcript update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptDelta {
    #[serde(rename = "assistant_message")]
    AssistantMessage { message: Message },
}

/// Continuation state passed between model steps (extracted from engine_state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelContinuationState {
    pub version: u32,
    pub kind: String,
    pub counters: ModelRuntimeCounters,
}

/// Runtime counters for model execution.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelRuntimeCounters {
    #[serde(default)]
    pub model_calls: u32,
    #[serde(default)]
    pub tool_calls: u32,
    #[serde(default)]
    pub consecutive_errors: u32,
    #[serde(default)]
    pub started_at: i64,
}

// ---- Engine state helpers ----

impl ModelContinuationState {
    pub fn extract(raw: Option<&serde_json::Value>) -> Option<Self> {
        raw.and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn ready(counters: ModelRuntimeCounters) -> serde_json::Value {
        serde_json::to_value(Self {
            version: 1,
            kind: "ready".into(),
            counters,
        })
        .unwrap_or_default()
    }
}

impl ModelRuntimeCounters {
    pub fn new() -> Self {
        Self {
            model_calls: 0,
            tool_calls: 0,
            consecutive_errors: 0,
            started_at: chrono::Utc::now().timestamp_millis(),
        }
    }
}
