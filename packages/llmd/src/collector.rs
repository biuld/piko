use futures::StreamExt;

use crate::gateway::{
    ErrorClass, InferenceError, InferenceEvent, InferenceExecution, InferenceItem, InferenceResult,
    OutputItemId, ToolCallId,
};

pub async fn collect_execution(
    mut execution: InferenceExecution,
) -> Result<InferenceResult, InferenceError> {
    let mut items = Vec::new();
    let mut usage = None;
    let mut checkpoint = None;
    let mut finish_reason = None;
    let mut auxiliary = Vec::new();
    while let Some(event) = execution.events.next().await {
        if finish_reason.is_some() {
            return Err(protocol_error("event observed after terminal outcome"));
        }
        match event {
            InferenceEvent::Cursor(_) => {}
            InferenceEvent::TextDelta { item_id, delta } => {
                append_text_item(&mut items, item_id, delta, ItemTextKind::Text)
            }
            InferenceEvent::RefusalDelta { item_id, delta } => {
                append_text_item(&mut items, item_id, delta, ItemTextKind::Refusal)
            }
            InferenceEvent::ReasoningDelta { item_id, delta } => {
                append_text_item(&mut items, item_id, delta, ItemTextKind::Reasoning)
            }
            InferenceEvent::ToolCallDelta {
                call_id,
                name,
                arguments_delta,
            } => append_tool_call(&mut items, call_id, name, arguments_delta),
            InferenceEvent::Usage(value) => usage = Some(value),
            InferenceEvent::Checkpoint(value) => {
                if checkpoint.replace(value).is_some() {
                    return Err(protocol_error(
                        "multiple checkpoints in one inference stream",
                    ));
                }
            }
            InferenceEvent::HostedActivity(value) => {
                auxiliary.push(crate::tools::InferenceAuxiliary::HostedActivity(value))
            }
            InferenceEvent::ApprovalRequired(value) => {
                auxiliary.push(crate::tools::InferenceAuxiliary::ApprovalRequired(value))
            }
            InferenceEvent::Source(value) => {
                auxiliary.push(crate::tools::InferenceAuxiliary::Source(value))
            }
            InferenceEvent::Citation(value) => {
                auxiliary.push(crate::tools::InferenceAuxiliary::Citation(value))
            }
            InferenceEvent::Artifact(value) => {
                auxiliary.push(crate::tools::InferenceAuxiliary::Artifact(value))
            }
            InferenceEvent::Completed(value) => finish_reason = Some(value),
            InferenceEvent::Error(error) => return Err(error),
        }
    }
    let finish_reason = finish_reason
        .ok_or_else(|| protocol_error("inference stream ended without a terminal event"))?;
    if checkpoint.is_some()
        && !matches!(
            &finish_reason,
            crate::gateway::FinishReason::Completed { .. }
        )
    {
        return Err(protocol_error(
            "checkpoint emitted for an incomplete inference",
        ));
    }
    Ok(InferenceResult {
        items,
        usage,
        finish_reason,
        checkpoint,
        auxiliary,
    })
}

fn protocol_error(message: &'static str) -> InferenceError {
    InferenceError::new(ErrorClass::Protocol, "gateway", "collect", message)
}

enum ItemTextKind {
    Text,
    Refusal,
    Reasoning,
}

fn append_text_item(
    items: &mut Vec<InferenceItem>,
    id: OutputItemId,
    delta: String,
    kind: ItemTextKind,
) {
    let existing = items.iter_mut().find(|item| match item {
        InferenceItem::Text { id: current, .. }
        | InferenceItem::Refusal { id: current, .. }
        | InferenceItem::Reasoning { id: current, .. } => current == &id,
        _ => false,
    });
    match existing {
        Some(InferenceItem::Text { text, .. })
        | Some(InferenceItem::Refusal { text, .. })
        | Some(InferenceItem::Reasoning { text, .. }) => text.push_str(&delta),
        _ => items.push(match kind {
            ItemTextKind::Text => InferenceItem::Text { text: delta, id },
            ItemTextKind::Refusal => InferenceItem::Refusal { text: delta, id },
            ItemTextKind::Reasoning => InferenceItem::Reasoning { text: delta, id },
        }),
    }
}

fn append_tool_call(
    items: &mut Vec<InferenceItem>,
    call_id: ToolCallId,
    name: String,
    delta: String,
) {
    if let Some(InferenceItem::ToolCall {
        name: current_name,
        arguments,
        ..
    }) = items.iter_mut().find(|item| {
        matches!(item, InferenceItem::ToolCall { call_id: current, .. } if current == &call_id)
    }) {
        if !name.is_empty() {
            *current_name = name;
        }
        arguments.push_str(&delta);
    } else {
        items.push(InferenceItem::ToolCall {
            name,
            arguments: delta,
            call_id,
        });
    }
}

#[cfg(test)]
mod tests {
    use futures::stream;

    use super::*;
    use crate::gateway::{FinishReason, InferenceExecution};

    fn execution(events: Vec<InferenceEvent>) -> InferenceExecution {
        InferenceExecution {
            events: Box::pin(stream::iter(events)),
            handle: None,
        }
    }

    #[tokio::test]
    async fn collector_rejects_duplicate_or_post_terminal_events() {
        let checkpoint: piko_protocol::OpaqueModelCheckpoint =
            serde_json::from_value(serde_json::json!("token")).unwrap();
        let duplicate = collect_execution(execution(vec![
            InferenceEvent::Checkpoint(checkpoint.clone()),
            InferenceEvent::Checkpoint(checkpoint),
            InferenceEvent::Completed(FinishReason::Completed {
                reason: "stop".into(),
            }),
        ]))
        .await
        .unwrap_err();
        assert_eq!(duplicate.class, ErrorClass::Protocol);

        let after_terminal = collect_execution(execution(vec![
            InferenceEvent::Completed(FinishReason::Completed {
                reason: "stop".into(),
            }),
            InferenceEvent::text("late"),
        ]))
        .await
        .unwrap_err();
        assert_eq!(after_terminal.class, ErrorClass::Protocol);
    }

    #[tokio::test]
    async fn collector_never_durably_accepts_checkpoint_for_cancelled_output() {
        let checkpoint = serde_json::from_value(serde_json::json!("token")).unwrap();
        let error = collect_execution(execution(vec![
            InferenceEvent::Checkpoint(checkpoint),
            InferenceEvent::Completed(FinishReason::Cancelled),
        ]))
        .await
        .unwrap_err();
        assert_eq!(error.class, ErrorClass::Protocol);
    }
}
