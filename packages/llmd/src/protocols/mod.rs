pub(crate) mod chat_completions;
pub(crate) mod responses;

#[cfg(test)]
pub(crate) mod tests_support;

use crate::gateway::{GatewayError, ModelEvent, ModelRequest, ModelResult};
use crate::target::ModelTarget;

pub(crate) trait ProtocolAdapter: Send + Sync {
    fn validate(&self, request: &ModelRequest, target: &ModelTarget) -> Result<(), GatewayError>;
    fn encode(
        &self,
        request: &ModelRequest,
        target: &ModelTarget,
        stream: bool,
    ) -> Result<serde_json::Value, GatewayError>;
    fn decode_response(
        &self,
        value: serde_json::Value,
        target: &ModelTarget,
    ) -> Result<ModelResult, GatewayError>;
    fn new_stream(&self, target: &ModelTarget) -> Box<dyn ProtocolStream>;
}

pub(crate) trait ProtocolStream: Send {
    fn push(&mut self, value: serde_json::Value) -> Result<Vec<ModelEvent>, GatewayError>;
    fn finish(&mut self) -> Result<Vec<ModelEvent>, GatewayError>;
    fn has_observable_output(&self) -> bool;
}

pub(crate) fn text_from_content(blocks: &[piko_protocol::ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            piko_protocol::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn input_text(content: &piko_protocol::MessageContent) -> String {
    match content {
        piko_protocol::MessageContent::String(text) => text.clone(),
        piko_protocol::MessageContent::Blocks(blocks) => text_from_content(blocks),
    }
}

pub(crate) fn instructions(request: &ModelRequest) -> String {
    request
        .run_prompt
        .blocks
        .iter()
        .map(|block| {
            format!(
                "[piko prompt block id={} authority={:?} trust={:?}]\n{}",
                block.id, block.authority, block.trust, block.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn usage(input: u64, output: u64, cache_read: u64) -> piko_protocol::Usage {
    piko_protocol::Usage {
        input,
        output,
        cache_read,
        cache_write: 0,
        total_tokens: input.saturating_add(output),
        cost: Default::default(),
    }
}
