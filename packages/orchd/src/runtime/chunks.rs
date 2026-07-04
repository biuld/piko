// ---- LLM chunk accumulator ----
//
// Collects LLM stream chunks (tool calls, usage, stop reason) while the
// main loop yields TextDelta/ThinkingDelta directly (typewriter effect).
// After the stream ends, `build_message()` assembles the final
// `Message::Assistant` from accumulated state.

use llmd::gateway::GatewayEvent;

use super::tool_executor::{ToolCallAggregator, ToolCallItem};
use crate::domain::model::step::ModelSpec;

use crate::domain::model::transcript::{AssistantContentBlock, Message};

pub(crate) struct LlmChunks {
    pub text: String,
    pub reasoning: String,
    tool_calls: ToolCallAggregator,
    usage: Option<crate::domain::model::transcript::MessageUsage>,
    pub stop_reason: String,
}

impl LlmChunks {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            tool_calls: ToolCallAggregator::new(),
            usage: None,
            stop_reason: "stop".into(),
        }
    }

    pub fn apply_non_delta(&mut self, event: GatewayEvent) {
        match event {
            GatewayEvent::ToolCallChunk {
                id,
                name,
                args_delta,
            } => {
                self.tool_calls.on_chunk(id, name, args_delta);
            }
            GatewayEvent::Usage(usage) => {
                self.usage = Some(usage);
            }
            GatewayEvent::Done(reason) => {
                self.stop_reason = reason;
            }
            GatewayEvent::Error(e) => {
                tracing::error!("Stream error: {e}");
                self.stop_reason = "error".into();
            }
            _ => {}
        }
    }

    pub fn build_message(&mut self, model: &ModelSpec) -> Message {
        let mut blocks = Vec::new();
        if !self.reasoning.is_empty() {
            blocks.push(AssistantContentBlock::Thinking {
                thinking: std::mem::take(&mut self.reasoning),
                thinking_signature: None,
            });
        }
        if !self.text.is_empty() {
            blocks.push(AssistantContentBlock::Text {
                text: std::mem::take(&mut self.text),
            });
        }
        if blocks.is_empty() {
            blocks.push(AssistantContentBlock::Text {
                text: String::new(),
            });
        }
        Message::Assistant {
            content: blocks,
            api: "openai-completions".into(),
            provider: model.provider.clone(),
            model: model.id.clone(),
            usage: self.usage.clone(),
            stop_reason: Some(self.stop_reason.clone()),
            error_message: None,
            timestamp: Some(chrono::Utc::now().timestamp_millis()),
        }
    }

    pub fn take_tool_calls(&mut self) -> Vec<ToolCallItem> {
        self.tool_calls
            .flush()
            .into_iter()
            .enumerate()
            .map(|(index, tool_call)| ToolCallItem {
                content_index: index as u32,
                tool_call_index: index as u32,
                id: tool_call.id,
                name: tool_call.name,
                arguments: tool_call.arguments,
            })
            .collect()
    }
}
