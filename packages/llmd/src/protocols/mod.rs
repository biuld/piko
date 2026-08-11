pub(crate) mod chat_completions;
pub(crate) mod responses;

#[cfg(test)]
pub(crate) mod tests_support;

use crate::checkpoint::ConversationPlan;
use crate::gateway::{
    ConversationItem, InferenceError, InferenceEvent, InferenceRequest, InferenceResult,
    OutputItemId, ToolCallId,
};
use crate::target::ModelTarget;

pub(crate) trait ProtocolAdapter: Send + Sync {
    fn validate(
        &self,
        request: &InferenceRequest,
        target: &ModelTarget,
    ) -> Result<(), InferenceError>;
    fn encode(
        &self,
        request: &InferenceRequest,
        target: &ModelTarget,
        plan: &ConversationPlan<'_>,
        stream: bool,
    ) -> Result<serde_json::Value, InferenceError>;
    fn decode_response(
        &self,
        value: serde_json::Value,
        target: &ModelTarget,
        request: &InferenceRequest,
    ) -> Result<InferenceResult, InferenceError>;
    fn new_stream(
        &self,
        target: &ModelTarget,
        request: &InferenceRequest,
    ) -> Box<dyn ProtocolStream>;
}

/// Provider coordinates used only while decoding one adapter response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdapterItemIdentity {
    pub response_id: Option<String>,
    pub item_id: Option<String>,
    pub call_id: Option<String>,
    pub output_index: Option<u32>,
    pub content_index: Option<u32>,
}

impl AdapterItemIdentity {
    pub(crate) fn semantic_item_id(&self, request: &InferenceRequest, kind: &str) -> OutputItemId {
        OutputItemId(format!(
            "out_{}",
            semantic_digest(
                request,
                kind,
                self.output_index.unwrap_or_default(),
                self.content_index.unwrap_or_default(),
            )
        ))
    }

    pub(crate) fn semantic_call_id(&self, request: &InferenceRequest) -> ToolCallId {
        self.call_id.clone().map(ToolCallId).unwrap_or_else(|| {
            ToolCallId(format!(
                "call_{}",
                semantic_digest(
                    request,
                    "tool_call",
                    self.output_index.unwrap_or_default(),
                    self.content_index.unwrap_or_default(),
                )
            ))
        })
    }
}

fn semantic_digest(
    request: &InferenceRequest,
    kind: &str,
    item_ordinal: u32,
    part_ordinal: u32,
) -> String {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    for value in [
        request.context.session_id.as_str(),
        request.context.agent_instance_id.as_str(),
        request.context.run_id.as_str(),
        request.context.step_id.as_str(),
        kind,
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    digest.update(item_ordinal.to_be_bytes());
    digest.update(part_ordinal.to_be_bytes());
    format!("{:x}", digest.finalize())[..24].to_string()
}

pub(crate) trait ProtocolStream: Send {
    fn push(&mut self, value: serde_json::Value) -> Result<Vec<InferenceEvent>, InferenceError>;
    fn finish(&mut self) -> Result<Vec<InferenceEvent>, InferenceError>;
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

pub(crate) fn instructions(request: &InferenceRequest) -> String {
    request
        .conversation
        .instructions
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

pub(crate) fn all_items<'a>(plan: &'a ConversationPlan<'a>) -> &'a [ConversationItem] {
    match plan {
        ConversationPlan::FullReplay { items } | ConversationPlan::OpaqueReplay { items, .. } => {
            items
        }
        ConversationPlan::Resume { suffix, .. } => suffix,
    }
}

pub(crate) fn usage(
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> piko_protocol::Usage {
    piko_protocol::Usage {
        input,
        output,
        cache_read,
        cache_write,
        total_tokens: input.saturating_add(output),
        units: Default::default(),
        cost: Default::default(),
    }
}
