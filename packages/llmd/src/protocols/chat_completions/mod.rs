use std::collections::HashMap;

use piko_protocol::{ContentBlock, Message, MessageContent};
use serde_json::{Value, json};

use crate::gateway::{
    ErrorClass, GatewayError, ItemIdentity, ModelEvent, ModelOutputMetadata, ModelRequest,
    ModelResult, SemanticItem, TerminalStatus,
};
use crate::protocols::{
    ProtocolAdapter, ProtocolStream, input_text, instructions, text_from_content, usage,
};
use crate::target::ModelTarget;

#[cfg(test)]
mod tests;

pub(crate) struct ChatCompletionsAdapter;

impl ProtocolAdapter for ChatCompletionsAdapter {
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
        let mut messages = Vec::with_capacity(request.transcript.len() + 1);
        let system = instructions(request);
        if !system.is_empty() {
            messages.push(json!({"role":"system","content":system}));
        }
        messages.extend(encode_messages(&request.transcript));
        let tools = request.tools.iter().map(|tool| json!({
            "type":"function",
            "function":{"name":tool.name,"description":tool.description,"parameters":tool.input_schema,"strict":false}
        })).collect::<Vec<_>>();
        let mut body = json!({"model":target.model,"messages":messages,"stream":stream});
        if stream {
            body["stream_options"] = json!({"include_usage":true});
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
            body["parallel_tool_calls"] = Value::Bool(true);
        }
        if let Some(effort) = &request.thinking {
            body["reasoning_effort"] = Value::String(effort.clone());
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
        Box::new(ChatStream::new(target.id.clone()))
    }
}

fn encode_message(message: &Message) -> Value {
    match message {
        Message::Context {
            content,
            trust,
            source,
            ..
        } => json!({
            "role":"user",
            "content":format!("[piko data-only context; authority=None; trust={trust:?}; source={}:{}]\n{}", source.kind, source.locator, input_text(content))
        }),
        Message::User { content, .. } => json!({"role":"user","content":encode_content(content)}),
        Message::Assistant { content, .. } => {
            json!({"role":"assistant","content":text_from_content(content)})
        }
        Message::ToolCall {
            id,
            name,
            arguments,
            ..
        } => json!({
            "role":"assistant","content":null,"tool_calls":[{
                "id":id,"type":"function","function":{"name":name,"arguments":serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())}
            }]
        }),
        Message::ToolResult {
            tool_call_id,
            content,
            ..
        } => json!({
            "role":"tool","tool_call_id":tool_call_id,"content":text_from_content(content)
        }),
    }
}

fn encode_messages(transcript: &[Message]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut index = 0;
    while index < transcript.len() {
        if let Message::Assistant { content, .. } = &transcript[index] {
            let mut tool_calls = Vec::new();
            let mut tool_results = Vec::new();
            index += 1;
            while let Some(message) = transcript.get(index) {
                match message {
                    Message::ToolCall { id, name, arguments, .. } => tool_calls.push(json!({
                        "id":id,"type":"function","function":{
                            "name":name,
                            "arguments":serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
                        }
                    })),
                    Message::ToolResult { .. } => tool_results.push(encode_message(message)),
                    _ => break,
                }
                index += 1;
            }
            let mut assistant = json!({"role":"assistant","content":text_from_content(content)});
            if !tool_calls.is_empty() {
                assistant["tool_calls"] = Value::Array(tool_calls);
            }
            messages.push(assistant);
            messages.extend(tool_results);
        } else {
            messages.push(encode_message(&transcript[index]));
            index += 1;
        }
    }
    messages
}

fn encode_content(content: &MessageContent) -> Value {
    match content {
        MessageContent::String(text) => Value::String(text.clone()),
        MessageContent::Blocks(blocks) => Value::Array(blocks.iter().filter_map(|block| match block {
            ContentBlock::Text { text } => Some(json!({"type":"text","text":text})),
            ContentBlock::Image { data, mime_type } => Some(json!({
                "type":"image_url","image_url":{"url":format!("data:{mime_type};base64,{data}")}
            })),
            ContentBlock::Thinking { .. } => None,
        }).collect()),
    }
}

fn decode_complete(value: Value, target: &ModelTarget) -> Result<ModelResult, GatewayError> {
    let choices = value
        .get("choices")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol(target, "choices is not an array"))?;
    let choice = choices
        .iter()
        .find(|choice| choice.get("index").and_then(Value::as_u64) == Some(0))
        .or_else(|| choices.first())
        .ok_or_else(|| protocol(target, "response has no choices"))?;
    if choices.len() > 1 && choice.get("index").and_then(Value::as_u64) != Some(0) {
        return Err(protocol(
            target,
            "multi-choice response has no choice index 0",
        ));
    }
    let message = choice
        .get("message")
        .ok_or_else(|| protocol(target, "choice is missing message"))?;
    let mut items = Vec::new();
    let identity = ItemIdentity {
        response_id: None,
        item_id: None,
        call_id: None,
        output_index: Some(0),
        content_index: None,
    };
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        items.push(SemanticItem::Text {
            text: text.into(),
            identity: identity.clone(),
        });
    }
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        items.push(SemanticItem::Refusal {
            text: refusal.into(),
            identity: identity.clone(),
        });
    }
    if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
        items.push(SemanticItem::Reasoning {
            text: reasoning.into(),
            identity: identity.clone(),
        });
    }
    let mut call_ids = Vec::new();
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let call_id =
                string(call, "id").ok_or_else(|| protocol(target, "tool call is missing id"))?;
            call_ids.push(call_id.clone());
            items.push(SemanticItem::FunctionCall {
                name: call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol(target, "tool call is missing function name"))?
                    .into(),
                arguments: call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                identity: ItemIdentity {
                    call_id: Some(call_id),
                    content_index: Some(index as u32),
                    ..identity.clone()
                },
            });
        }
    }
    let reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol(target, "choice is missing finish_reason"))?
        .to_string();
    let usage = value.get("usage").map(decode_usage);
    Ok(ModelResult {
        items,
        usage,
        status: TerminalStatus::Completed { reason },
        output_metadata: output_metadata(call_ids),
    })
}

fn output_metadata(tool_call_ids: Vec<String>) -> ModelOutputMetadata {
    ModelOutputMetadata {
        continuation: (!tool_call_ids.is_empty())
            .then_some(piko_protocol::ModelContinuation::ChatCompletions { tool_call_ids }),
    }
}

fn decode_usage(value: &Value) -> piko_protocol::Usage {
    usage(
        value
            .get("prompt_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        value
            .get("completion_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        value
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    )
}

#[derive(Default)]
struct ToolState {
    id: Option<String>,
    name: String,
    pending_arguments: String,
}

pub(super) struct ChatStream {
    target: String,
    finish_reason: Option<String>,
    tools: HashMap<u32, ToolState>,
    observable: bool,
    usage_seen: bool,
    finished: bool,
}

impl ChatStream {
    pub(super) fn new(target: String) -> Self {
        Self {
            target,
            finish_reason: None,
            tools: HashMap::new(),
            observable: false,
            usage_seen: false,
            finished: false,
        }
    }
    fn error(&self, message: impl Into<String>) -> GatewayError {
        GatewayError::new(
            ErrorClass::Protocol,
            &self.target,
            "decode_chat_stream",
            message,
        )
    }
}

impl ProtocolStream for ChatStream {
    fn push(&mut self, value: Value) -> Result<Vec<ModelEvent>, GatewayError> {
        if self.finished {
            return Err(self.error("chunk received after stream terminal"));
        }
        let mut events = Vec::new();
        if let Some(usage_value) = value.get("usage").filter(|value| !value.is_null()) {
            if self.usage_seen {
                return Err(self.error("duplicate usage chunk"));
            }
            self.usage_seen = true;
            events.push(ModelEvent::Usage(decode_usage(usage_value)));
        }
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| self.error("chunk choices is not an array"))?;
        for choice in choices {
            let index = choice
                .get("index")
                .and_then(Value::as_u64)
                .ok_or_else(|| self.error("choice is missing index"))?;
            if index != 0 {
                continue;
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                if self.finish_reason.is_some() {
                    return Err(self.error("duplicate finish reason"));
                }
                self.finish_reason = Some(reason.into());
            }
            let Some(delta) = choice.get("delta") else {
                continue;
            };
            let identity = ItemIdentity {
                response_id: None,
                item_id: None,
                call_id: None,
                output_index: Some(0),
                content_index: None,
            };
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                self.observable = true;
                events.push(ModelEvent::TextDelta {
                    delta: text.into(),
                    identity: identity.clone(),
                });
            }
            if let Some(refusal) = delta.get("refusal").and_then(Value::as_str) {
                self.observable = true;
                events.push(ModelEvent::RefusalDelta {
                    delta: refusal.into(),
                    identity: identity.clone(),
                });
            }
            if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
                self.observable = true;
                events.push(ModelEvent::ReasoningDelta {
                    delta: reasoning.into(),
                    identity: identity.clone(),
                });
            }
            if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for call in calls {
                    let call_index = call
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| self.error("tool call delta is missing index"))?
                        as u32;
                    let state = self.tools.entry(call_index).or_default();
                    if let Some(id) = string(call, "id") {
                        if state.id.as_ref().is_some_and(|old| old != &id) {
                            return Err(self.error("tool call index changed id"));
                        }
                        state.id = Some(id);
                    }
                    if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                        state.name.push_str(name);
                    }
                    let arguments = call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if state.id.is_none() {
                        state.pending_arguments.push_str(&arguments);
                        continue;
                    }
                    let arguments_delta = if state.pending_arguments.is_empty() {
                        arguments
                    } else {
                        state.pending_arguments.push_str(&arguments);
                        std::mem::take(&mut state.pending_arguments)
                    };
                    self.observable = true;
                    events.push(ModelEvent::FunctionCallDelta {
                        name: state.name.clone(),
                        arguments_delta,
                        identity: ItemIdentity {
                            call_id: state.id.clone(),
                            content_index: Some(call_index),
                            ..identity.clone()
                        },
                    });
                }
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<ModelEvent>, GatewayError> {
        if self.finished {
            return Err(self.error("duplicate stream terminal"));
        }
        self.finished = true;
        let reason = self
            .finish_reason
            .clone()
            .ok_or_else(|| self.error("stream ended before a finish reason"))?;
        for (index, state) in &self.tools {
            if state.id.is_none() {
                return Err(self.error(format!("tool call index {index} has no id")));
            }
            if !state.pending_arguments.is_empty() {
                return Err(
                    self.error(format!("tool call index {index} has undelivered arguments"))
                );
            }
        }
        let mut calls = self
            .tools
            .iter()
            .filter_map(|(index, state)| state.id.clone().map(|id| (*index, id)))
            .collect::<Vec<_>>();
        calls.sort_by_key(|(index, _)| *index);
        Ok(vec![
            ModelEvent::OutputMetadata(output_metadata(
                calls.into_iter().map(|(_, id)| id).collect(),
            )),
            ModelEvent::Completed(TerminalStatus::Completed { reason }),
        ])
    }

    fn has_observable_output(&self) -> bool {
        self.observable
    }
}

fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn protocol(target: &ModelTarget, message: impl Into<String>) -> GatewayError {
    GatewayError::new(
        ErrorClass::Protocol,
        &target.id,
        "decode_chat_completions",
        message,
    )
}
