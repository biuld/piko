use serde_json::Value;

use super::decode_items::decode_message_item;
use super::decode_upstream::upstream_activity;
use super::support::{
    EncryptedReasoningItem, ResponsesCheckpointOutput, ResponsesContinuation, decode_usage,
    output_checkpoint, protocol, string,
};
use crate::gateway::{
    FinishReason, InferenceError, InferenceItem, InferenceRequest, InferenceResult,
};
use crate::protocols::AdapterItemIdentity;
use crate::target::ModelTarget;
use crate::tools::{InferenceAuxiliary, UpstreamActivityStatus};

pub(super) fn decode_complete(
    value: Value,
    target: &ModelTarget,
    request: &InferenceRequest,
) -> Result<InferenceResult, InferenceError> {
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
    let mut auxiliary = Vec::new();
    for (index, item) in output.iter().enumerate() {
        let kind =
            string(item, "type").ok_or_else(|| protocol(target, "output item is missing type"))?;
        let item_id = string(item, "id");
        if let Some(id) = &item_id {
            item_ids.push(id.clone());
        }
        let identity = AdapterItemIdentity {
            response_id: Some(response_id.clone()),
            item_id,
            call_id: string(item, "call_id"),
            output_index: Some(index as u32),
            content_index: None,
        };
        match kind.as_str() {
            "message" => decode_message_item(item, &identity, &mut items, target, request)?,
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
                let id = identity.semantic_item_id(request, "reasoning");
                items.push(InferenceItem::Reasoning { text, id });
            }
            "function_call" => {
                let call_id = string(item, "call_id")
                    .ok_or_else(|| protocol(target, "function call is missing call_id"))?;
                call_ids.push(call_id.clone());
                let mut identity = identity;
                identity.call_id = Some(call_id);
                items.push(InferenceItem::ToolCall {
                    name: string(item, "name")
                        .ok_or_else(|| protocol(target, "function call is missing name"))?,
                    arguments: string(item, "arguments").unwrap_or_default(),
                    call_id: identity.semantic_call_id(request),
                });
            }
            "function_call_output" => items.push(InferenceItem::ToolResult {
                output: string(item, "output").unwrap_or_default(),
                call_id: identity.semantic_call_id(request),
            }),
            other => {
                if let Some(activity) = upstream_activity(
                    item,
                    other,
                    target,
                    request,
                    UpstreamActivityStatus::Completed,
                ) {
                    auxiliary.push(InferenceAuxiliary::UpstreamActivity(activity));
                } else {
                    return Err(protocol(
                        target,
                        format!("unsupported required output item type {other}"),
                    ));
                }
            }
        }
    }
    let usage = value.get("usage").map(decode_usage);
    let status = match status.as_str() {
        "completed" => FinishReason::Completed {
            reason: "stop".into(),
        },
        "incomplete" => FinishReason::Incomplete {
            reason: value
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        },
        "failed" => FinishReason::Failed {
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
    let checkpoint = matches!(&status, FinishReason::Completed { .. })
        .then(|| {
            let assistant_reasoning = items
                .iter()
                .filter_map(|item| match item {
                    InferenceItem::Reasoning { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<String>();
            let assistant_text = items
                .iter()
                .filter_map(|item| match item {
                    InferenceItem::Text { text, .. } | InferenceItem::Refusal { text, .. } => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect::<String>();
            output_checkpoint(
                request,
                target,
                ResponsesCheckpointOutput {
                    continuation: ResponsesContinuation {
                        response_id,
                        output_item_ids: item_ids,
                        call_ids,
                        encrypted_reasoning,
                    },
                    assistant_reasoning,
                    assistant_text,
                },
            )
        })
        .transpose()?;
    Ok(InferenceResult {
        items,
        usage,
        finish_reason: status,
        checkpoint,
        auxiliary,
    })
}
