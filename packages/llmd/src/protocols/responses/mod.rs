use piko_protocol::{ContentBlock, Message, MessageContent};
use serde_json::{Value, json};

use crate::gateway::{ErrorClass, GatewayError, ModelRequest, ModelResult};
mod decode;
mod support;

use decode::{ResponsesStream, decode_complete};

#[cfg(test)]
mod tests;
use crate::protocols::{
    ProtocolAdapter, ProtocolStream, input_text, instructions, text_from_content,
};
use crate::target::ModelTarget;
use support::decode_continuation;

pub(crate) struct ResponsesAdapter;

impl ProtocolAdapter for ResponsesAdapter {
    fn validate(&self, request: &ModelRequest, target: &ModelTarget) -> Result<(), GatewayError> {
        target.validate(request)
    }

    fn encode(
        &self,
        request: &ModelRequest,
        target: &ModelTarget,
        stream: bool,
    ) -> Result<Value, GatewayError> {
        self.validate(request, target)?;
        let continuation = target.responses_continuation().ok_or_else(|| {
            GatewayError::new(
                crate::gateway::ErrorClass::Protocol,
                &target.id,
                "encode",
                "Responses adapter received a non-Responses target",
            )
        })?;
        let (previous_response_id, input, store, include_encrypted) = match continuation {
            crate::modeling::ResponsesContinuationPolicy::PreviousResponseId => {
                let (previous_response_id, transcript) =
                    continuation_suffix(&request.transcript, target)?;
                let input = transcript
                    .iter()
                    .map(|message| encode_message(message, target, false))
                    .collect::<Result<Vec<_>, _>>()?;
                (previous_response_id, input, Some(true), false)
            }
            crate::modeling::ResponsesContinuationPolicy::EncryptedReasoning => (
                None,
                encrypted_replay_input(&request.transcript, target)?,
                Some(false),
                true,
            ),
            crate::modeling::ResponsesContinuationPolicy::StatelessReplay => (
                None,
                plaintext_replay_input(&request.transcript, target)?,
                None,
                false,
            ),
        };
        let tools = request
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                    "strict": false
                })
            })
            .collect::<Vec<_>>();
        let mut body = json!({
            "model": target.model,
            "instructions": instructions(request),
            "input": input,
            "stream": stream
        });
        if let Some(store) = store {
            body["store"] = Value::Bool(store);
        }
        if let Some(response_id) = previous_response_id {
            body["previous_response_id"] = Value::String(response_id);
        }
        if include_encrypted {
            body["include"] = json!(["reasoning.encrypted_content"]);
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
            body["parallel_tool_calls"] = Value::Bool(true);
        }
        if let Some(effort) = request.thinking.as_deref().filter(|value| *value != "none") {
            body["reasoning"] = json!({ "effort": effort, "summary": "auto" });
        }
        Ok(body)
    }

    fn decode_response(
        &self,
        value: Value,
        target: &ModelTarget,
    ) -> Result<ModelResult, GatewayError> {
        decode_complete(value, target)
    }

    fn new_stream(&self, target: &ModelTarget) -> Box<dyn ProtocolStream> {
        Box::new(ResponsesStream::new(target.id.clone()))
    }
}

fn plaintext_replay_input(
    transcript: &[Message],
    target: &ModelTarget,
) -> Result<Vec<Value>, GatewayError> {
    let mut input = Vec::new();
    for message in transcript {
        if let Message::Assistant { content, .. } = message {
            input.extend(content.iter().filter_map(|block| match block {
                ContentBlock::Thinking { thinking, .. } => Some(json!({
                    "type": "reasoning",
                    "content": [{ "type": "reasoning_text", "text": thinking }]
                })),
                _ => None,
            }));
        }
        input.push(encode_message(message, target, true)?);
    }
    Ok(input)
}

fn continuation_suffix<'a>(
    transcript: &'a [Message],
    target: &ModelTarget,
) -> Result<(Option<String>, &'a [Message]), GatewayError> {
    for (index, message) in transcript.iter().enumerate().rev() {
        if let Message::Assistant {
            continuation: Some(envelope),
            ..
        } = message
            && let Some(continuation) = decode_continuation(envelope, target)?
        {
            return Ok((Some(continuation.response_id), &transcript[index + 1..]));
        }
    }
    Ok((None, transcript))
}

fn encrypted_replay_input(
    transcript: &[Message],
    target: &ModelTarget,
) -> Result<Vec<Value>, GatewayError> {
    let mut input = Vec::new();
    for message in transcript {
        let encrypted = match message {
            Message::Assistant {
                continuation: Some(continuation),
                ..
            } => decode_continuation(continuation, target)?
                .map(|state| state.encrypted_reasoning)
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        input.extend(encrypted.iter().map(|item| {
            json!({
                "type": "reasoning",
                "id": item.item_id,
                "encrypted_content": item.encrypted_content,
                "summary": []
            })
        }));
        input.push(encode_message(message, target, !encrypted.is_empty())?);
    }
    Ok(input)
}

fn encode_message(
    message: &Message,
    target: &ModelTarget,
    allow_encrypted_reasoning: bool,
) -> Result<Value, GatewayError> {
    Ok(match message {
        Message::Context {
            content,
            trust,
            source,
            ..
        } => json!({
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": format!(
                    "[piko data-only context; authority=None; trust={trust:?}; source={}:{}]\n{}",
                    source.kind, source.locator, input_text(content)
                )
            }]
        }),
        Message::User { content, .. } => json!({
            "role": "user",
            "content": encode_content(content)
        }),
        Message::Assistant { content, .. } => {
            if !allow_encrypted_reasoning
                && content
                    .iter()
                    .any(|block| matches!(block, ContentBlock::Thinking { .. }))
            {
                return Err(GatewayError::new(
                    ErrorClass::UnsupportedCapability,
                    &target.id,
                    "encode_responses",
                    "reasoning output requires retained Responses continuation",
                ));
            }
            json!({
                "role": "assistant",
                "content": content.iter().filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(json!({ "type": "input_text", "text": text })),
                    ContentBlock::Image { data, mime_type } => Some(json!({
                        "type": "input_image", "image_url": format!("data:{mime_type};base64,{data}")
                    })),
                    ContentBlock::Thinking { .. } => None,
                }).collect::<Vec<_>>()
            })
        }
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => json!({
            "type": "function_call",
            "call_id": id,
            "name": name,
            "arguments": serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
        }),
        Message::ToolResult {
            tool_call_id,
            content,
            ..
        } => json!({
            "type": "function_call_output",
            "call_id": tool_call_id,
            "output": text_from_content(content)
        }),
    })
}

fn encode_content(content: &MessageContent) -> Vec<Value> {
    match content {
        MessageContent::String(text) => vec![json!({ "type": "input_text", "text": text })],
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(json!({ "type": "input_text", "text": text })),
                ContentBlock::Image { data, mime_type } => Some(json!({
                    "type": "input_image", "image_url": format!("data:{mime_type};base64,{data}")
                })),
                ContentBlock::Thinking { .. } => None,
            })
            .collect(),
    }
}
