use piko_orchd_api::AgentApiError;
use piko_protocol::messages::{ContentBlock, Message, MessageContent};

/// Fail-closed provider preflight using a documented conservative estimator.
pub(super) fn enforce_context_budget(
    prompt: &piko_protocol::SemanticRunPrompt,
    transcript: &[Message],
    tools: &[piko_protocol::ToolDef],
    context_window: u64,
    output_reserve: u64,
    reasoning_enabled: bool,
) -> Result<(), AgentApiError> {
    let prompt_tokens = serialized_tokens(prompt);
    let tool_tokens = serialized_tokens(tools).saturating_add(tools.len() as u64 * 32);
    let reasoning_reserve = if reasoning_enabled { output_reserve } else { 0 };
    let safety_margin = (context_window / 50).max(256);
    let fixed_tokens = prompt_tokens
        .saturating_add(tool_tokens)
        .saturating_add(output_reserve)
        .saturating_add(reasoning_reserve)
        .saturating_add(safety_margin);
    if fixed_tokens >= context_window {
        return Err(AgentApiError::ContextBudgetExceeded(format!(
            "fixed estimate prompt={prompt_tokens}, tools={tool_tokens}, output={output_reserve}, reasoning={reasoning_reserve}, margin={safety_margin}, window={context_window}"
        )));
    }

    let transcript_tokens = transcript.iter().map(message_tokens).sum::<u64>();
    let total = fixed_tokens.saturating_add(transcript_tokens);
    if total > context_window {
        return Err(AgentApiError::ContextBudgetExceeded(format!(
            "estimated request={total}, fixed={fixed_tokens}, transcript={transcript_tokens}, window={context_window}; compaction required"
        )));
    }
    Ok(())
}

fn serialized_tokens<T: serde::Serialize + ?Sized>(value: &T) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| (bytes.len() as u64).div_ceil(3))
        .unwrap_or(u64::MAX)
}

fn message_tokens(message: &Message) -> u64 {
    let content = match message {
        Message::Context { content, .. } | Message::User { content, .. } => {
            message_content_tokens(content)
        }
        Message::Assistant { content, .. } | Message::ToolResult { content, .. } => {
            blocks_tokens(content)
        }
        Message::ToolCall {
            name, arguments, ..
        } => text_tokens(name).saturating_add(serialized_tokens(arguments)),
    };
    content.saturating_add(16)
}

fn message_content_tokens(content: &MessageContent) -> u64 {
    match content {
        MessageContent::String(text) => text_tokens(text),
        MessageContent::Blocks(blocks) => blocks_tokens(blocks),
    }
}

fn blocks_tokens(blocks: &[ContentBlock]) -> u64 {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => text_tokens(text),
            ContentBlock::Thinking {
                thinking,
                thinking_signature,
            } => text_tokens(thinking)
                .saturating_add(thinking_signature.as_deref().map(text_tokens).unwrap_or(0)),
            // Base64 tokenization varies. One token per encoded byte plus
            // framing is deliberately conservative across providers.
            ContentBlock::Image { data, mime_type } => data
                .len()
                .saturating_add(mime_type.len())
                .saturating_add(512) as u64,
        })
        .sum()
}

fn text_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(3)
}
