use std::collections::HashMap;

use piko_protocol::{ContentBlock, MessageContent};
use serde_json::{Value, json};

use crate::gateway::{
    ErrorClass, FinishReason, InferenceError, InferenceEvent, InferenceItem, InferenceRequest,
    InferenceResult,
};
use crate::protocols::{
    AdapterItemIdentity, ProtocolAdapter, ProtocolStream, all_items, input_text, instructions,
    text_from_content, usage,
};
use crate::target::ModelTarget;
use crate::{checkpoint::ConversationPlan, gateway::ConversationItemKind};

#[cfg(test)]
mod tests;

pub(crate) struct ChatCompletionsAdapter;

impl ProtocolAdapter for ChatCompletionsAdapter {
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
        let mut messages = Vec::with_capacity(request.conversation.items.len() + 1);
        let system = instructions(request);
        if !system.is_empty() {
            messages.push(json!({"role":"system","content":system}));
        }
        messages.extend(encode_messages(all_items(plan)));
        let tools = request.tools.iter().filter_map(crate::tools::InferenceTool::caller).map(|tool| json!({
            "type":"function",
            "function":{"name":tool.name,"description":tool.description,"parameters":tool.input_schema,"strict":false}
        })).collect::<Vec<_>>();
        let mut body = json!({"model":target.model,"messages":messages,"stream":stream});
        if stream {
            body["stream_options"] = json!({"include_usage":true});
        }
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        if let Some(parallel) = request.options.parallel_tools {
            body["parallel_tool_calls"] = Value::Bool(parallel);
        }
        match &request.options.tool_choice {
            crate::gateway::ToolChoice::Auto => {}
            crate::gateway::ToolChoice::None => body["tool_choice"] = json!("none"),
            crate::gateway::ToolChoice::Required => body["tool_choice"] = json!("required"),
            crate::gateway::ToolChoice::Specific(name) => {
                body["tool_choice"] = json!({"type":"function","function":{"name":name}});
            }
        }
        if let Some(effort) = &request.options.reasoning_effort
            && let Some(effort) = target.reasoning_effort(effort)
        {
            body["reasoning_effort"] = Value::String(effort);
        }
        if let Some(max_tokens) = request.options.max_output_tokens {
            body["max_completion_tokens"] = Value::Number(max_tokens.into());
        }
        if let Some(intent) = &request.options.structured_output {
            body["response_format"] = json!({
                "type":"json_schema",
                "json_schema":{"name":"piko_output","strict":intent.strict,"schema":intent.schema}
            });
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
        Box::new(ChatStream::new(target.id.clone(), request.clone()))
    }
}

fn encode_message(item: &ConversationItemKind) -> Value {
    match item {
        ConversationItemKind::Context {
            content,
            trust,
            source,
        } => json!({
            "role":"user",
            "content":format!("[piko data-only context; authority=None; trust={trust:?}; source={}:{}]\n{}", source.kind, source.locator, input_text(content))
        }),
        ConversationItemKind::User { content } => {
            json!({"role":"user","content":encode_content(content)})
        }
        ConversationItemKind::Assistant { content } => {
            json!({"role":"assistant","content":text_from_content(content)})
        }
        ConversationItemKind::ToolCall {
            call_id,
            name,
            arguments,
        } => json!({
            "role":"assistant","content":null,"tool_calls":[{
                "id":call_id.0,"type":"function","function":{"name":name,"arguments":serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())}
            }]
        }),
        ConversationItemKind::ToolResult {
            call_id, content, ..
        } => json!({
            "role":"tool","tool_call_id":call_id.0,"content":text_from_content(content)
        }),
        ConversationItemKind::HostedActivity(activity) => json!({
            "role":"assistant","content":format!("[hosted tool activity: {activity:?}]")
        }),
        ConversationItemKind::Source(source) => json!({
            "role":"assistant","content":format!("[source: {source:?}]")
        }),
        ConversationItemKind::Citation(citation) => json!({
            "role":"assistant","content":format!("[citation: {citation:?}]")
        }),
        ConversationItemKind::Artifact(artifact) => json!({
            "role":"assistant","content":format!("[generated artifact: {artifact:?}]")
        }),
    }
}

fn encode_messages(transcript: &[crate::gateway::ConversationItem]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut index = 0;
    while index < transcript.len() {
        if let ConversationItemKind::Assistant { content } = &transcript[index].kind {
            let mut tool_calls = Vec::new();
            let mut tool_results = Vec::new();
            index += 1;
            while let Some(item) = transcript.get(index) {
                match &item.kind {
                    ConversationItemKind::ToolCall { call_id, name, arguments } => tool_calls.push(json!({
                        "id":call_id.0,"type":"function","function":{
                            "name":name,
                            "arguments":serde_json::to_string(arguments).unwrap_or_else(|_| "{}".into())
                        }
                    })),
                    ConversationItemKind::ToolResult { .. } => tool_results.push(encode_message(&item.kind)),
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
            messages.push(encode_message(&transcript[index].kind));
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
            other => Some(json!({"type":"text","text":other.text_projection()})),
        }).collect()),
    }
}

fn decode_complete(
    value: Value,
    target: &ModelTarget,
    request: &InferenceRequest,
) -> Result<InferenceResult, InferenceError> {
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
    let identity = AdapterItemIdentity {
        response_id: None,
        item_id: None,
        call_id: None,
        output_index: Some(0),
        content_index: None,
    };
    if let Some(text) = message.get("content").and_then(Value::as_str) {
        items.push(InferenceItem::Text {
            text: text.into(),
            id: identity.semantic_item_id(request, "text"),
        });
    }
    if let Some(refusal) = message.get("refusal").and_then(Value::as_str) {
        items.push(InferenceItem::Refusal {
            text: refusal.into(),
            id: identity.semantic_item_id(request, "refusal"),
        });
    }
    if let Some(reasoning) = message.get("reasoning_content").and_then(Value::as_str) {
        items.push(InferenceItem::Reasoning {
            text: reasoning.into(),
            id: identity.semantic_item_id(request, "reasoning"),
        });
    }
    if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
        for (index, call) in calls.iter().enumerate() {
            let call_id =
                string(call, "id").ok_or_else(|| protocol(target, "tool call is missing id"))?;
            items.push(InferenceItem::ToolCall {
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
                call_id: AdapterItemIdentity {
                    call_id: Some(call_id),
                    content_index: Some(index as u32),
                    ..identity.clone()
                }
                .semantic_call_id(request),
            });
        }
    }
    let reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol(target, "choice is missing finish_reason"))?
        .to_string();
    let usage = value.get("usage").map(decode_usage);
    Ok(InferenceResult {
        items,
        usage,
        finish_reason: FinishReason::Completed { reason },
        checkpoint: None,
        auxiliary: Vec::new(),
    })
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
    request: InferenceRequest,
    finish_reason: Option<String>,
    tools: HashMap<u32, ToolState>,
    observable: bool,
    usage_seen: bool,
    finished: bool,
}

impl ChatStream {
    pub(super) fn new(target: String, request: InferenceRequest) -> Self {
        Self {
            target,
            request,
            finish_reason: None,
            tools: HashMap::new(),
            observable: false,
            usage_seen: false,
            finished: false,
        }
    }
    fn error(&self, message: impl Into<String>) -> InferenceError {
        InferenceError::new(
            ErrorClass::Protocol,
            &self.target,
            "decode_chat_stream",
            message,
        )
    }
}

impl ProtocolStream for ChatStream {
    fn push(&mut self, value: Value) -> Result<Vec<InferenceEvent>, InferenceError> {
        if self.finished {
            return Err(self.error("chunk received after stream terminal"));
        }
        let mut events = Vec::new();
        if let Some(usage_value) = value.get("usage").filter(|value| !value.is_null()) {
            if self.usage_seen {
                return Err(self.error("duplicate usage chunk"));
            }
            self.usage_seen = true;
            events.push(InferenceEvent::Usage(decode_usage(usage_value)));
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
            let identity = AdapterItemIdentity {
                response_id: None,
                item_id: None,
                call_id: None,
                output_index: Some(0),
                content_index: None,
            };
            if let Some(text) = delta.get("content").and_then(Value::as_str) {
                self.observable = true;
                events.push(InferenceEvent::TextDelta {
                    delta: text.into(),
                    item_id: identity.semantic_item_id(&self.request, "text"),
                });
            }
            if let Some(refusal) = delta.get("refusal").and_then(Value::as_str) {
                self.observable = true;
                events.push(InferenceEvent::RefusalDelta {
                    delta: refusal.into(),
                    item_id: identity.semantic_item_id(&self.request, "refusal"),
                });
            }
            if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
                self.observable = true;
                events.push(InferenceEvent::ReasoningDelta {
                    delta: reasoning.into(),
                    item_id: identity.semantic_item_id(&self.request, "reasoning"),
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
                    events.push(InferenceEvent::ToolCallDelta {
                        name: state.name.clone(),
                        arguments_delta,
                        call_id: AdapterItemIdentity {
                            call_id: state.id.clone(),
                            content_index: Some(call_index),
                            ..identity.clone()
                        }
                        .semantic_call_id(&self.request),
                    });
                }
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<InferenceEvent>, InferenceError> {
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
        Ok(vec![InferenceEvent::Completed(FinishReason::Completed {
            reason,
        })])
    }

    fn has_observable_output(&self) -> bool {
        self.observable
    }
}

fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn protocol(target: &ModelTarget, message: impl Into<String>) -> InferenceError {
    InferenceError::new(
        ErrorClass::Protocol,
        &target.id,
        "decode_chat_completions",
        message,
    )
}
