// ---- LLM chunk accumulator ----
//
// Collects LLM stream chunks (tool calls, usage, stop reason) while the
// main loop yields TextDelta/ThinkingDelta directly (typewriter effect).
// After the stream ends, `build_message()` assembles the final
// `Message::Assistant` from accumulated state.

use std::collections::HashMap;

use llmd::gateway::GatewayEvent;

use crate::domain::model::step::ModelSpec;
use crate::domain::model::transcript::{ContentBlock, Message};

pub(crate) struct LlmChunks {
    pub text: String,
    pub reasoning: String,
    tool_calls: HashMap<usize, (String, String, serde_json::Value)>,
    usage: Option<crate::domain::model::transcript::MessageUsage>,
    pub stop_reason: String,
}

impl LlmChunks {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            reasoning: String::new(),
            tool_calls: HashMap::new(),
            usage: None,
            stop_reason: "stop".into(),
        }
    }

    pub fn apply_non_delta(&mut self, event: GatewayEvent) {
        match event {
            GatewayEvent::ToolCallStart {
                index,
                id,
                name,
                args,
            } => {
                self.tool_calls.insert(index, (id, name, args));
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
            blocks.push(ContentBlock::Thinking {
                thinking: std::mem::take(&mut self.reasoning),
                thinking_signature: None,
            });
        }
        if !self.text.is_empty() {
            blocks.push(ContentBlock::Text {
                text: std::mem::take(&mut self.text),
            });
        }
        let mut sorted: Vec<_> = self.tool_calls.keys().copied().collect();
        sorted.sort_unstable();
        for idx in sorted {
            if let Some((id, name, args)) = self.tool_calls.remove(&idx) {
                blocks.push(ContentBlock::ToolCall {
                    id,
                    name,
                    arguments: args,
                    partial_json: None,
                });
            }
        }
        if blocks.is_empty() {
            blocks.push(ContentBlock::Text {
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
}
