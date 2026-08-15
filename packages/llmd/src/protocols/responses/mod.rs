use piko_protocol::{ContentBlock, MessageContent};
use serde_json::{Value, json};

use crate::checkpoint::ConversationPlan;
use crate::gateway::{ConversationItem, ConversationItemKind};
use crate::gateway::{ErrorClass, InferenceError, InferenceRequest, InferenceResult};
use crate::modeling::ResponsesVariant;
mod decode;
mod decode_items;
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
    fn validate(
        &self,
        request: &InferenceRequest,
        target: &ModelTarget,
    ) -> Result<(), InferenceError> {
        target.validate(request)
    }

    fn encode(
        &self,
        request: &InferenceRequest,
        target: &ModelTarget,
        plan: &ConversationPlan<'_>,
        stream: bool,
    ) -> Result<Value, InferenceError> {
        self.validate(request, target)?;
        let continuation = target.responses_continuation().ok_or_else(|| {
            InferenceError::new(
                crate::gateway::ErrorClass::Protocol,
                &target.id,
                "encode",
                "Responses adapter received a non-Responses target",
            )
        })?;
        let (previous_response_id, mut input, mut store, mut include_encrypted) = match plan {
            ConversationPlan::Resume { checkpoint, suffix } => {
                let continuation = decode_continuation(checkpoint, target)?;
                let input = suffix
                    .iter()
                    .map(|item| encode_message(&item.kind, target, false))
                    .collect::<Result<Vec<_>, _>>()?;
                (Some(continuation.response_id), input, Some(true), false)
            }
            ConversationPlan::OpaqueReplay { checkpoint, items } => {
                let continuation = decode_continuation(checkpoint, target)?;
                (
                    None,
                    encrypted_replay_input(items, &continuation, target)?,
                    Some(false),
                    true,
                )
            }
            ConversationPlan::FullReplay { items } => (
                None,
                plaintext_replay_input(items, target)?,
                matches!(
                    continuation,
                    crate::modeling::ResponsesContinuationPolicy::PreviousResponseId
                )
                .then_some(true),
                false,
            ),
        };
        let tools = caller_tools(request);
        let lite = target.responses_variant() == Some(ResponsesVariant::CodexLite);
        let request_instructions = instructions(request);
        if lite {
            let mut prefix = vec![json!({
                "type": "additional_tools",
                "role": "developer",
                "tools": tools
            })];
            if !request_instructions.is_empty() {
                prefix.push(json!({
                    "type": "message",
                    "role": "developer",
                    "content": [{
                        "type": "input_text",
                        "text": request_instructions
                    }]
                }));
            }
            prefix.append(&mut input);
            input = prefix;
            store = Some(false);
            include_encrypted = true;
        }
        let mut body = json!({
            "model": target.model,
            "input": input,
            "stream": stream
        });
        if !lite {
            body["instructions"] = Value::String(request_instructions);
        }
        if let Some(store) = store {
            body["store"] = Value::Bool(store);
        }
        if let Some(response_id) = previous_response_id {
            body["previous_response_id"] = Value::String(response_id);
        }
        if include_encrypted {
            body["include"] = json!(["reasoning.encrypted_content"]);
        }
        if !lite && !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if lite {
            body["parallel_tool_calls"] = Value::Bool(false);
        } else if let Some(parallel) = request.options.parallel_tools {
            body["parallel_tool_calls"] = Value::Bool(parallel);
        }
        match &request.options.tool_choice {
            crate::gateway::ToolChoice::Auto if lite => body["tool_choice"] = json!("auto"),
            crate::gateway::ToolChoice::Auto => {}
            crate::gateway::ToolChoice::None => body["tool_choice"] = json!("none"),
            crate::gateway::ToolChoice::Required => body["tool_choice"] = json!("required"),
            crate::gateway::ToolChoice::Specific(name) => {
                body["tool_choice"] = json!({"type":"function","name":name});
            }
        }
        if lite {
            let mut reasoning = json!({
                "context": "all_turns",
                "summary": "auto"
            });
            if let Some(effort) = &request.options.reasoning_effort
                && let Some(effort) = target.reasoning_effort(effort)
            {
                reasoning["effort"] = Value::String(effort);
            }
            body["reasoning"] = reasoning;
        } else if let Some(effort) = &request.options.reasoning_effort
            && let Some(effort) = target.reasoning_effort(effort)
        {
            body["reasoning"] = json!({ "effort": effort, "summary": "auto" });
        }
        if let Some(max_tokens) = request.options.max_output_tokens {
            body["max_output_tokens"] = Value::Number(max_tokens.into());
        }
        if let Some(intent) = &request.options.structured_output {
            body["text"] = json!({"format":{
                "type":"json_schema",
                "name":"piko_output",
                "strict":intent.strict,
                "schema":intent.schema
            }});
        }
        Ok(body)
    }

    fn decode_response(
        &self,
        value: Value,
        target: &ModelTarget,
        request: &InferenceRequest,
    ) -> Result<InferenceResult, InferenceError> {
        decode_complete(value, target, request)
    }

    fn new_stream(
        &self,
        target: &ModelTarget,
        request: &InferenceRequest,
    ) -> Box<dyn ProtocolStream> {
        Box::new(ResponsesStream::new(target.clone(), request.clone()))
    }
}

fn caller_tools(request: &InferenceRequest) -> Vec<Value> {
    request
        .tools
        .iter()
        .filter_map(crate::tools::InferenceTool::caller)
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
                "strict": false
            })
        })
        .collect()
}

fn plaintext_replay_input(
    transcript: &[ConversationItem],
    target: &ModelTarget,
) -> Result<Vec<Value>, InferenceError> {
    let mut input = Vec::new();
    for item in transcript {
        if let ConversationItemKind::Assistant { content } = &item.kind {
            input.extend(content.iter().filter_map(|block| match block {
                ContentBlock::Thinking { thinking, .. } => Some(json!({
                    "type": "reasoning",
                    "content": [{ "type": "reasoning_text", "text": thinking }]
                })),
                _ => None,
            }));
        }
        input.push(encode_message(&item.kind, target, true)?);
    }
    Ok(input)
}

fn encrypted_replay_input(
    transcript: &[ConversationItem],
    continuation: &support::ResponsesContinuation,
    target: &ModelTarget,
) -> Result<Vec<Value>, InferenceError> {
    let mut input = Vec::new();
    input.extend(continuation.encrypted_reasoning.iter().map(|item| {
        json!({
            "type": "reasoning",
            "id": item.item_id,
            "encrypted_content": item.encrypted_content,
            "summary": []
        })
    }));
    for item in transcript {
        input.push(encode_message(&item.kind, target, true)?);
    }
    Ok(input)
}

fn encode_message(
    message: &ConversationItemKind,
    target: &ModelTarget,
    allow_encrypted_reasoning: bool,
) -> Result<Value, InferenceError> {
    Ok(match message {
        ConversationItemKind::Context {
            content,
            trust,
            source,
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
        ConversationItemKind::User { content } => json!({
            "role": "user",
            "content": encode_content(content)
        }),
        ConversationItemKind::Assistant { content } => {
            if !allow_encrypted_reasoning
                && content
                    .iter()
                    .any(|block| matches!(block, ContentBlock::Thinking { .. }))
            {
                return Err(InferenceError::new(
                    ErrorClass::UnsupportedCapability,
                    &target.id,
                    "encode_responses",
                    "reasoning output requires retained Responses continuation",
                ));
            }
            json!({
                "role": "assistant",
                "content": content.iter().filter_map(|block| match block {
                    ContentBlock::Thinking { .. } => None,
                    ContentBlock::Text { text } => {
                        Some(json!({ "type": "output_text", "text": text }))
                    }
                    other => Some(json!({
                        "type": "output_text",
                        "text": other.text_projection()
                    })),
                }).collect::<Vec<_>>()
            })
        }
        ConversationItemKind::ToolCall {
            call_id,
            name,
            arguments,
        } => json!({
            "type": "function_call",
            "call_id": call_id.0,
            "name": name,
            "arguments": serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
        }),
        ConversationItemKind::ToolResult {
            call_id, content, ..
        } => json!({
            "type": "function_call_output",
            "call_id": call_id.0,
            "output": text_from_content(content)
        }),
        ConversationItemKind::UpstreamActivity(activity) => json!({
            "role":"assistant","content":[{"type":"output_text","text":format!("[upstream tool activity: {activity:?}]")}]
        }),
        ConversationItemKind::Source(source) => json!({
            "role":"assistant","content":[{"type":"output_text","text":format!("[source: {source:?}]")}]
        }),
        ConversationItemKind::Citation(citation) => json!({
            "role":"assistant","content":[{"type":"output_text","text":format!("[citation: {citation:?}]")}]
        }),
        ConversationItemKind::Artifact(artifact) => json!({
            "role":"assistant","content":[{"type":"output_text","text":format!("[generated artifact: {artifact:?}]")}]
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
                other => Some(json!({ "type": "input_text", "text": other.text_projection() })),
            })
            .collect(),
    }
}
