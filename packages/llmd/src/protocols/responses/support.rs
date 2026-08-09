use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::checkpoint::AdapterCheckpoint;
use crate::gateway::{ErrorClass, InferenceError, InferenceRequest};
use crate::target::ModelTarget;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct EncryptedReasoningItem {
    pub item_id: String,
    pub encrypted_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ResponsesContinuation {
    pub response_id: String,
    pub output_item_ids: Vec<String>,
    pub call_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub encrypted_reasoning: Vec<EncryptedReasoningItem>,
}

pub(super) struct ResponsesCheckpointOutput {
    pub continuation: ResponsesContinuation,
    pub assistant_reasoning: String,
    pub assistant_text: String,
}

pub(super) fn decode_continuation(
    checkpoint: &AdapterCheckpoint,
    target: &ModelTarget,
) -> Result<ResponsesContinuation, InferenceError> {
    serde_json::from_value(checkpoint.payload.clone())
        .map_err(|error| protocol(target, format!("invalid Responses continuation: {error}")))
}

pub(super) fn string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

pub(super) fn decode_usage(value: &Value) -> piko_protocol::Usage {
    crate::protocols::usage(
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

pub(super) fn protocol(target: &ModelTarget, message: impl Into<String>) -> InferenceError {
    InferenceError::new(
        ErrorClass::Protocol,
        &target.id,
        "decode_responses",
        message,
    )
}

pub(super) fn output_checkpoint(
    request: &InferenceRequest,
    target: &ModelTarget,
    output: ResponsesCheckpointOutput,
) -> Result<piko_protocol::OpaqueModelCheckpoint, InferenceError> {
    let state = serde_json::to_value(output.continuation)
        .expect("Responses continuation contains only serializable values");
    crate::checkpoint::encode(
        target,
        &request.conversation,
        crate::checkpoint::assistant_output_digest(
            &output.assistant_reasoning,
            &output.assistant_text,
        ),
        state,
    )
}
