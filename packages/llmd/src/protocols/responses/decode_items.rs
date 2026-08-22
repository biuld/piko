use serde_json::Value;

use super::support::{protocol, string};
use crate::gateway::{InferenceError, InferenceItem, InferenceRequest};
use crate::protocols::AdapterItemIdentity;
use crate::target::ModelTarget;

#[derive(Debug, Clone)]
pub(super) struct StreamItem {
    pub identity: AdapterItemIdentity,
    pub name: String,
    pub kind: StreamItemKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamItemKind {
    Message,
    Reasoning,
    FunctionCall,
    Upstream,
}

pub(super) fn decode_message_item(
    item: &Value,
    identity: &AdapterItemIdentity,
    items: &mut Vec<InferenceItem>,
    target: &ModelTarget,
    request: &InferenceRequest,
) -> Result<(), InferenceError> {
    let content = item
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol(target, "message content is not an array"))?;
    for (index, part) in content.iter().enumerate() {
        let mut identity = identity.clone();
        identity.content_index = Some(index as u32);
        match string(part, "type").as_deref() {
            Some("output_text") => items.push(InferenceItem::Text {
                text: string(part, "text").unwrap_or_default(),
                id: identity.semantic_item_id(request, "text"),
            }),
            Some("refusal") => items.push(InferenceItem::Refusal {
                text: string(part, "refusal")
                    .or_else(|| string(part, "text"))
                    .unwrap_or_default(),
                id: identity.semantic_item_id(request, "refusal"),
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
