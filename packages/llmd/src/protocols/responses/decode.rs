use std::collections::HashMap;

use serde_json::Value;

use super::support::{EncryptedReasoningItem, output_metadata, protocol, string};
use crate::gateway::{
    ErrorClass, GatewayError, ItemIdentity, ModelEvent, ModelOutputMetadata, ModelResult,
    SemanticItem, TerminalStatus,
};
use crate::protocols::{ProtocolStream, usage};
use crate::target::ModelTarget;

pub(super) fn decode_complete(
    value: Value,
    target: &ModelTarget,
) -> Result<ModelResult, GatewayError> {
    let response_id =
        string(&value, "id").ok_or_else(|| protocol(target, "response is missing id"))?;
    let status =
        string(&value, "status").ok_or_else(|| protocol(target, "response is missing status"))?;
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol(target, "response output is not an array"))?;
    let mut items = Vec::new();
    let mut item_ids = Vec::new();
    let mut call_ids = Vec::new();
    let mut encrypted_reasoning = Vec::new();
    for (index, item) in output.iter().enumerate() {
        let kind =
            string(item, "type").ok_or_else(|| protocol(target, "output item is missing type"))?;
        let item_id = string(item, "id");
        if let Some(id) = &item_id {
            item_ids.push(id.clone());
        }
        let identity = ItemIdentity {
            response_id: Some(response_id.clone()),
            item_id,
            call_id: string(item, "call_id"),
            output_index: Some(index as u32),
            content_index: None,
        };
        match kind.as_str() {
            "message" => decode_message_item(item, &identity, &mut items, target)?,
            "reasoning" => {
                if let Some(encrypted_content) = string(item, "encrypted_content") {
                    let item_id = identity.item_id.clone().ok_or_else(|| {
                        protocol(target, "encrypted reasoning item is missing id")
                    })?;
                    encrypted_reasoning.push(EncryptedReasoningItem {
                        item_id,
                        encrypted_content,
                    });
                }
                let text = ["content", "summary"]
                    .into_iter()
                    .flat_map(|field| {
                        item.get(field)
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                    })
                    .filter_map(|part| string(part, "text"))
                    .collect::<Vec<_>>()
                    .join("\n");
                items.push(SemanticItem::Reasoning { text, identity });
            }
            "function_call" => {
                let call_id = string(item, "call_id")
                    .ok_or_else(|| protocol(target, "function call is missing call_id"))?;
                call_ids.push(call_id.clone());
                let mut identity = identity;
                identity.call_id = Some(call_id);
                items.push(SemanticItem::FunctionCall {
                    name: string(item, "name")
                        .ok_or_else(|| protocol(target, "function call is missing name"))?,
                    arguments: string(item, "arguments").unwrap_or_default(),
                    identity,
                });
            }
            "function_call_output" => items.push(SemanticItem::FunctionResult {
                output: string(item, "output").unwrap_or_default(),
                identity,
            }),
            other => {
                return Err(protocol(
                    target,
                    format!("unsupported required output item type {other}"),
                ));
            }
        }
    }
    let usage = value.get("usage").map(decode_usage);
    let status = match status.as_str() {
        "completed" => TerminalStatus::Completed {
            reason: "stop".into(),
        },
        "incomplete" => TerminalStatus::Incomplete {
            reason: value
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        "failed" => TerminalStatus::Failed {
            message: value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(crate::redaction::sanitize_diagnostic)
                .unwrap_or_else(|| "upstream response failed".into()),
        },
        other => {
            return Err(protocol(
                target,
                format!("non-terminal response status {other}"),
            ));
        }
    };
    Ok(ModelResult {
        items,
        usage,
        status,
        output_metadata: output_metadata(response_id, item_ids, call_ids, encrypted_reasoning),
    })
}

fn decode_message_item(
    item: &Value,
    identity: &ItemIdentity,
    items: &mut Vec<SemanticItem>,
    target: &ModelTarget,
) -> Result<(), GatewayError> {
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol(target, "message content is not an array"))?;
    for (index, part) in content.iter().enumerate() {
        let mut identity = identity.clone();
        identity.content_index = Some(index as u32);
        match string(part, "type").as_deref() {
            Some("output_text") => items.push(SemanticItem::Text {
                text: string(part, "text").unwrap_or_default(),
                identity,
            }),
            Some("refusal") => items.push(SemanticItem::Refusal {
                text: string(part, "refusal")
                    .or_else(|| string(part, "text"))
                    .unwrap_or_default(),
                identity,
            }),
            Some(other) => {
                return Err(protocol(
                    target,
                    format!("unsupported required content type {other}"),
                ));
            }
            None => return Err(protocol(target, "content part is missing type")),
        }
    }
    Ok(())
}

fn decode_usage(value: &Value) -> piko_protocol::Usage {
    usage(
        value
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        value
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        value
            .pointer("/input_tokens_details/cached_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    )
}

#[derive(Debug, Clone)]
struct StreamItem {
    identity: ItemIdentity,
    name: String,
    kind: StreamItemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamItemKind {
    Message,
    Reasoning,
    FunctionCall,
}

pub(super) struct ResponsesStream {
    target: String,
    response_id: Option<String>,
    items: HashMap<u32, StreamItem>,
    terminal: bool,
    observable: bool,
    encrypted_reasoning: Vec<EncryptedReasoningItem>,
}

impl ResponsesStream {
    pub(super) fn new(target: String) -> Self {
        Self {
            target,
            response_id: None,
            items: HashMap::new(),
            terminal: false,
            observable: false,
            encrypted_reasoning: Vec::new(),
        }
    }

    fn error(&self, message: impl Into<String>) -> GatewayError {
        GatewayError::new(
            ErrorClass::Protocol,
            &self.target,
            "decode_responses_stream",
            message,
        )
    }

    fn identity(
        &self,
        event: &Value,
        expected: StreamItemKind,
    ) -> Result<ItemIdentity, GatewayError> {
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

    fn output_metadata(&self) -> Result<ModelOutputMetadata, GatewayError> {
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
        Ok(output_metadata(
            response_id,
            output_item_ids,
            call_ids,
            self.encrypted_reasoning.clone(),
        ))
    }
}

impl ProtocolStream for ResponsesStream {
    fn push(&mut self, value: Value) -> Result<Vec<ModelEvent>, GatewayError> {
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
                    Some(other) => {
                        return Err(
                            self.error(format!("unsupported required output item type {other}"))
                        );
                    }
                    None => return Err(self.error("output item is missing type")),
                };
                let identity = ItemIdentity {
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
                events.push(ModelEvent::TextDelta {
                    delta: string(&value, "delta").unwrap_or_default(),
                    identity: self.identity(&value, StreamItemKind::Message)?,
                });
            }
            "response.refusal.delta" => {
                self.observable = true;
                events.push(ModelEvent::RefusalDelta {
                    delta: string(&value, "delta").unwrap_or_default(),
                    identity: self.identity(&value, StreamItemKind::Message)?,
                });
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                self.observable = true;
                events.push(ModelEvent::ReasoningDelta {
                    delta: string(&value, "delta").unwrap_or_default(),
                    identity: self.identity(&value, StreamItemKind::Reasoning)?,
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
                events.push(ModelEvent::FunctionCallDelta {
                    name,
                    arguments_delta: string(&value, "delta").unwrap_or_default(),
                    identity,
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
                    events.push(ModelEvent::Usage(decode_usage(usage_value)));
                }
                let status = match kind.as_str() {
                    "response.completed" => TerminalStatus::Completed {
                        reason: "stop".into(),
                    },
                    "response.incomplete" => TerminalStatus::Incomplete {
                        reason: response
                            .pointer("/incomplete_details/reason")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    },
                    _ => TerminalStatus::Failed {
                        message: response
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .map(crate::redaction::sanitize_diagnostic)
                            .unwrap_or_else(|| "upstream response failed".into()),
                    },
                };
                events.push(ModelEvent::OutputMetadata(self.output_metadata()?));
                events.push(ModelEvent::Completed(status));
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
            other => {
                return Err(self.error(format!("unsupported required stream event type {other}")));
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<ModelEvent>, GatewayError> {
        if !self.terminal {
            return Err(self.error("stream ended before a terminal event"));
        }
        Ok(Vec::new())
    }

    fn has_observable_output(&self) -> bool {
        self.observable
    }
}
