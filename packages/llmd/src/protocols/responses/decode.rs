use std::collections::HashMap;

use serde_json::Value;

use super::decode_items::{StreamItem, StreamItemKind};
use super::decode_upstream::upstream_activity;
use super::support::{
    EncryptedReasoningItem, ResponsesCheckpointOutput, ResponsesContinuation, decode_usage,
    output_checkpoint, string,
};
use crate::gateway::{ErrorClass, FinishReason, InferenceError, InferenceEvent, InferenceRequest};
use crate::protocols::{AdapterItemIdentity, ProtocolStream};
use crate::target::ModelTarget;
use crate::tools::UpstreamActivityStatus;

pub(super) struct ResponsesStream {
    target: ModelTarget,
    request: InferenceRequest,
    response_id: Option<String>,
    items: HashMap<u32, StreamItem>,
    terminal: bool,
    observable: bool,
    encrypted_reasoning: Vec<EncryptedReasoningItem>,
    assistant_text: String,
    assistant_reasoning: String,
}

impl ResponsesStream {
    pub(super) fn new(target: ModelTarget, request: InferenceRequest) -> Self {
        Self {
            target,
            request,
            response_id: None,
            items: HashMap::new(),
            terminal: false,
            observable: false,
            encrypted_reasoning: Vec::new(),
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
        }
    }

    fn error(&self, message: impl Into<String>) -> InferenceError {
        InferenceError::new(
            ErrorClass::Protocol,
            &self.target.id,
            "decode_responses_stream",
            message,
        )
    }

    fn identity(
        &self,
        event: &Value,
        expected: StreamItemKind,
    ) -> Result<AdapterItemIdentity, InferenceError> {
        let output_index = event
            .get("output_index")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .ok_or_else(|| self.error("semantic delta is missing output_index"))?;
        let item = self
            .items
            .get(&output_index)
            .ok_or_else(|| self.error("semantic delta references an unknown output item"))?;
        if item.kind != expected {
            return Err(self.error("semantic delta does not match its output item type"));
        }
        let mut identity = item.identity.clone();
        identity.content_index = event
            .get("content_index")
            .and_then(Value::as_u64)
            .map(|value| value as u32);
        Ok(identity)
    }

    fn retain_encrypted_reasoning(&mut self, item: EncryptedReasoningItem) {
        if let Some(existing) = self
            .encrypted_reasoning
            .iter_mut()
            .find(|existing| existing.item_id == item.item_id)
        {
            *existing = item;
        } else {
            self.encrypted_reasoning.push(item);
        }
    }

    fn output_checkpoint(&self) -> Result<piko_protocol::OpaqueModelCheckpoint, InferenceError> {
        let response_id = self
            .response_id
            .clone()
            .ok_or_else(|| self.error("terminal response has no created response id"))?;
        let mut items = self.items.iter().collect::<Vec<_>>();
        items.sort_by_key(|(index, _)| **index);
        let output_item_ids = items
            .iter()
            .filter_map(|(_, item)| item.identity.item_id.clone())
            .collect();
        let call_ids = items
            .iter()
            .filter_map(|(_, item)| item.identity.call_id.clone())
            .collect();
        output_checkpoint(
            &self.request,
            &self.target,
            ResponsesCheckpointOutput {
                continuation: ResponsesContinuation {
                    response_id,
                    output_item_ids,
                    call_ids,
                    encrypted_reasoning: self.encrypted_reasoning.clone(),
                },
                assistant_reasoning: self.assistant_reasoning.clone(),
                assistant_text: self.assistant_text.clone(),
            },
        )
    }

    fn is_upstream_lifecycle_notification(&self, event_type: &str) -> bool {
        let Some(event_type) = event_type.strip_prefix("response.") else {
            return false;
        };
        let Some((activity_type, phase)) = event_type.rsplit_once('.') else {
            return false;
        };
        matches!(phase, "in_progress" | "searching" | "completed")
            && self
                .target
                .upstream_tool_for_activity(activity_type)
                .is_some()
    }
}

impl ProtocolStream for ResponsesStream {
    fn push(&mut self, value: Value) -> Result<Vec<InferenceEvent>, InferenceError> {
        if self.terminal {
            return Err(self.error("event received after terminal event"));
        }
        let kind =
            string(&value, "type").ok_or_else(|| self.error("stream event is missing type"))?;
        let mut events = Vec::new();
        match kind.as_str() {
            "response.created" => {
                if self.response_id.is_some() {
                    return Err(self.error("duplicate response.created"));
                }
                self.response_id = value
                    .pointer("/response/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if self.response_id.is_none() {
                    return Err(self.error("response.created is missing response id"));
                }
            }
            "response.output_item.added" => {
                if self.response_id.is_none() {
                    return Err(self.error("output item arrived before response.created"));
                }
                let index = value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| self.error("output item is missing output_index"))?
                    as u32;
                let item = value
                    .get("item")
                    .ok_or_else(|| self.error("output item is missing item"))?;
                let item_kind = match string(item, "type").as_deref() {
                    Some("message") => StreamItemKind::Message,
                    Some("reasoning") => StreamItemKind::Reasoning,
                    Some("function_call") => StreamItemKind::FunctionCall,
                    Some(other) if self.target.upstream_tool_for_activity(other).is_some() => {
                        StreamItemKind::Upstream
                    }
                    Some(other) => {
                        return Err(
                            self.error(format!("unsupported required output item type {other}"))
                        );
                    }
                    None => return Err(self.error("output item is missing type")),
                };
                let identity = AdapterItemIdentity {
                    response_id: self.response_id.clone(),
                    item_id: string(item, "id"),
                    call_id: string(item, "call_id"),
                    output_index: Some(index),
                    content_index: None,
                };
                if self.items.contains_key(&index) {
                    return Err(self.error("duplicate output item index"));
                }
                self.items.insert(
                    index,
                    StreamItem {
                        identity,
                        name: string(item, "name").unwrap_or_default(),
                        kind: item_kind,
                    },
                );
                if matches!(item_kind, StreamItemKind::Upstream) {
                    self.observable = true;
                    let item_type = string(item, "type").unwrap_or_default();
                    if let Some(activity) = upstream_activity(
                        item,
                        &item_type,
                        &self.target,
                        &self.request,
                        UpstreamActivityStatus::Started,
                    ) {
                        events.push(InferenceEvent::UpstreamActivity(activity));
                    }
                }
                if matches!(item_kind, StreamItemKind::Reasoning)
                    && let Some(encrypted_content) = string(item, "encrypted_content")
                {
                    let item_id = string(item, "id")
                        .ok_or_else(|| self.error("encrypted reasoning item is missing id"))?;
                    self.retain_encrypted_reasoning(EncryptedReasoningItem {
                        item_id,
                        encrypted_content,
                    });
                }
            }
            "response.output_text.delta" => {
                self.observable = true;
                let identity = self.identity(&value, StreamItemKind::Message)?;
                let delta = string(&value, "delta").unwrap_or_default();
                self.assistant_text.push_str(&delta);
                events.push(InferenceEvent::TextDelta {
                    delta,
                    item_id: identity.semantic_item_id(&self.request, "text"),
                });
            }
            "response.refusal.delta" => {
                self.observable = true;
                let identity = self.identity(&value, StreamItemKind::Message)?;
                let delta = string(&value, "delta").unwrap_or_default();
                self.assistant_text.push_str(&delta);
                events.push(InferenceEvent::RefusalDelta {
                    delta,
                    item_id: identity.semantic_item_id(&self.request, "refusal"),
                });
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                self.observable = true;
                let identity = self.identity(&value, StreamItemKind::Reasoning)?;
                let delta = string(&value, "delta").unwrap_or_default();
                self.assistant_reasoning.push_str(&delta);
                events.push(InferenceEvent::ReasoningDelta {
                    delta,
                    item_id: identity.semantic_item_id(&self.request, "reasoning"),
                });
            }
            "response.function_call_arguments.delta" => {
                self.observable = true;
                let identity = self.identity(&value, StreamItemKind::FunctionCall)?;
                let name = identity
                    .output_index
                    .and_then(|index| self.items.get(&index))
                    .map(|item| item.name.clone())
                    .unwrap_or_default();
                events.push(InferenceEvent::ToolCallDelta {
                    name,
                    arguments_delta: string(&value, "delta").unwrap_or_default(),
                    call_id: identity.semantic_call_id(&self.request),
                });
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                self.terminal = true;
                let response = value
                    .get("response")
                    .ok_or_else(|| self.error("terminal event is missing response"))?;
                let terminal_id = string(response, "id")
                    .ok_or_else(|| self.error("terminal response is missing id"))?;
                if self.response_id.as_deref() != Some(terminal_id.as_str()) {
                    return Err(self.error("terminal response id does not match response.created"));
                }
                if let Some(usage_value) = response.get("usage") {
                    events.push(InferenceEvent::Usage(decode_usage(usage_value)));
                }
                let status = match kind.as_str() {
                    "response.completed" => FinishReason::Completed {
                        reason: "stop".into(),
                    },
                    "response.incomplete" => FinishReason::Incomplete {
                        reason: response
                            .pointer("/incomplete_details/reason")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    },
                    _ => FinishReason::Failed {
                        message: response
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(crate::redaction::sanitize_diagnostic)
                            .unwrap_or_else(|| "upstream response failed".into()),
                    },
                };
                if matches!(&status, FinishReason::Completed { .. }) {
                    events.push(InferenceEvent::Checkpoint(self.output_checkpoint()?));
                }
                events.push(InferenceEvent::Completed(status));
            }
            "response.output_item.done" => {
                let index = value
                    .get("output_index")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| self.error("output item done is missing output_index"))?
                    as u32;
                let stored = self
                    .items
                    .get(&index)
                    .ok_or_else(|| self.error("output item done references an unknown item"))?;
                let item = value
                    .get("item")
                    .ok_or_else(|| self.error("output item done event is missing item"))?;
                let done_kind = match string(item, "type").as_deref() {
                    Some("message") => StreamItemKind::Message,
                    Some("reasoning") => StreamItemKind::Reasoning,
                    Some("function_call") => StreamItemKind::FunctionCall,
                    Some(other) if self.target.upstream_tool_for_activity(other).is_some() => {
                        StreamItemKind::Upstream
                    }
                    Some(other) => {
                        return Err(
                            self.error(format!("unsupported required output item type {other}"))
                        );
                    }
                    None => return Err(self.error("output item done is missing type")),
                };
                if stored.kind != done_kind {
                    return Err(self.error("output item changed type before completion"));
                }
                if matches!(done_kind, StreamItemKind::Upstream) {
                    self.observable = true;
                    let item_type = string(item, "type").unwrap_or_default();
                    if let Some(activity) = upstream_activity(
                        item,
                        &item_type,
                        &self.target,
                        &self.request,
                        UpstreamActivityStatus::Completed,
                    ) {
                        events.push(InferenceEvent::UpstreamActivity(activity));
                    }
                }
                if matches!(done_kind, StreamItemKind::Reasoning)
                    && let Some(encrypted_content) = string(item, "encrypted_content")
                {
                    let item_id = string(item, "id")
                        .ok_or_else(|| self.error("encrypted reasoning item is missing id"))?;
                    self.retain_encrypted_reasoning(EncryptedReasoningItem {
                        item_id,
                        encrypted_content,
                    });
                }
            }
            // Additive lifecycle notifications that do not carry semantic deltas.
            "response.in_progress"
            | "response.content_part.added"
            | "response.content_part.done"
            | "response.output_text.done"
            | "response.refusal.done"
            | "response.function_call_arguments.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_text.done" => {}
            other if self.is_upstream_lifecycle_notification(other) => {}
            other => {
                return Err(self.error(format!("unsupported required stream event type {other}")));
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<InferenceEvent>, InferenceError> {
        if !self.terminal {
            return Err(self.error("stream ended before a terminal event"));
        }
        Ok(Vec::new())
    }

    fn has_observable_output(&self) -> bool {
        self.observable
    }
}
