use serde_json::Value;

use crate::gateway::{ErrorClass, GatewayError, ModelOutputMetadata};
use crate::target::ModelTarget;

pub(super) fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn protocol(target: &ModelTarget, message: impl Into<String>) -> GatewayError {
    GatewayError::new(
        ErrorClass::Protocol,
        &target.id,
        "decode_responses",
        message,
    )
}

pub(super) fn output_metadata(
    response_id: String,
    output_item_ids: Vec<String>,
    call_ids: Vec<String>,
    encrypted_reasoning: Vec<piko_protocol::EncryptedReasoningItem>,
) -> ModelOutputMetadata {
    ModelOutputMetadata {
        continuation: Some(piko_protocol::ModelContinuation::Responses {
            response_id,
            output_item_ids,
            call_ids,
            encrypted_reasoning,
        }),
    }
}
