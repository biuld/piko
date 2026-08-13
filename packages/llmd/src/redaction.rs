pub(crate) fn sanitize_diagnostic(message: &str) -> String {
    sanitize_sensitive_text(message)
        .chars()
        .take(1_024)
        .collect()
}

/// Redact common credential shapes without applying diagnostic-length limits.
pub(crate) fn sanitize_sensitive_text(message: &str) -> String {
    let mut sanitized = message
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    for word in message.split_whitespace() {
        let jwt_like = word.len() > 32 && word.matches('.').count() == 2;
        if word.starts_with("sk-") || jwt_like {
            sanitized = sanitized.replace(word, "[REDACTED]");
        }
    }
    let mut search_from = 0;
    while let Some(offset) = sanitized[search_from..]
        .to_ascii_lowercase()
        .find("bearer ")
    {
        let start = search_from + offset;
        let token_start = start + "bearer ".len();
        let token_end = sanitized[token_start..]
            .find(char::is_whitespace)
            .map(|offset| token_start + offset)
            .unwrap_or(sanitized.len());
        sanitized.replace_range(token_start..token_end, "[REDACTED]");
        search_from = token_start + "[REDACTED]".len();
    }
    sanitized
}

/// Remove provider continuation resources and credential-shaped values from
/// local prompt-debug snapshots. These values may have originated inside an
/// opaque checkpoint and must not become observable again after wire
/// encoding.
pub(crate) fn redact_model_input(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let contains_encrypted_state = object.contains_key("encrypted_content");
            serde_json::Value::Object(
                object
                    .iter()
                    .map(|(key, value)| {
                        let private = matches!(
                            key.to_ascii_lowercase().as_str(),
                            "authorization"
                                | "api_key"
                                | "previous_response_id"
                                | "encrypted_content"
                                | "checkpoint"
                        ) || (contains_encrypted_state && key == "id");
                        (
                            key.clone(),
                            if private {
                                serde_json::Value::String("[REDACTED]".into())
                            } else {
                                redact_model_input(value)
                            },
                        )
                    })
                    .collect(),
            )
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(redact_model_input).collect())
        }
        serde_json::Value::String(value) => serde_json::Value::String(sanitize_diagnostic(value)),
        other => other.clone(),
    }
}

pub(crate) fn semantic_model_input(
    request: &crate::gateway::InferenceRequest,
) -> serde_json::Value {
    let items = request
        .conversation
        .items
        .iter()
        .map(|item| {
            serde_json::json!({
                "id": item.id.0,
                "kind": item.kind,
            })
        })
        .collect::<Vec<_>>();
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name(),
                "executionLocus": tool.locus(),
            })
        })
        .collect::<Vec<_>>();
    redact_model_input(&serde_json::json!({
        "model": {"provider":request.model.provider,"model":request.model.model},
        "instructions": request.conversation.instructions,
        "items": items,
        "tools": tools,
    }))
}

pub(crate) fn semantic_inference_options(
    options: &crate::gateway::InferenceOptions,
) -> serde_json::Value {
    let delivery = match options.delivery {
        crate::gateway::DeliveryMode::Streaming => "streaming",
        crate::gateway::DeliveryMode::Assembled => "assembled",
    };
    let tool_choice = match &options.tool_choice {
        crate::gateway::ToolChoice::Auto => serde_json::json!({"kind":"auto"}),
        crate::gateway::ToolChoice::None => serde_json::json!({"kind":"none"}),
        crate::gateway::ToolChoice::Required => serde_json::json!({"kind":"required"}),
        crate::gateway::ToolChoice::Specific(name) => {
            serde_json::json!({"kind":"specific","name":name})
        }
    };
    serde_json::json!({
        "delivery": delivery,
        "reasoningEffort": options.reasoning_effort.as_ref().map(|value| value.as_str()),
        "toolChoice": tool_choice,
        "parallelTools": options.parallel_tools,
        "maxOutputTokens": options.max_output_tokens,
        "structuredOutput": options.structured_output.as_ref().map(|intent| {
            serde_json::json!({"strict":intent.strict,"schema":intent.schema})
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_bearer_and_key_shapes() {
        let value = sanitize_diagnostic("Bearer secret-token sk-example");
        assert_eq!(value, "Bearer [REDACTED] [REDACTED]");
    }

    #[test]
    fn sensitive_text_redaction_preserves_non_secret_tail() {
        let value = sanitize_sensitive_text(&format!("Bearer secret {} tail", "x".repeat(2_000)));
        assert!(value.starts_with("Bearer [REDACTED]"));
        assert!(value.ends_with("tail"));
    }

    #[test]
    fn redacts_checkpoint_derived_wire_state() {
        let value = serde_json::json!({
            "previous_response_id":"resp_secret",
            "input":[{"type":"reasoning","id":"rs_secret","encrypted_content":"cipher"}],
            "call_id":"semantic_call"
        });
        let redacted = redact_model_input(&value);
        let text = redacted.to_string();
        assert!(!text.contains("resp_secret"));
        assert!(!text.contains("rs_secret"));
        assert!(!text.contains("cipher"));
        assert!(text.contains("semantic_call"));
    }
}
