use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::gateway::{
    ErrorClass, InferenceError, InferenceEventStream, InferenceRequest, ModelDescriptor, ModelRef,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueExecutionHandle(#[serde(deserialize_with = "deserialize_bounded_token")] String);

impl std::fmt::Debug for OpaqueExecutionHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpaqueExecutionHandle([REDACTED])")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueEventCursor(#[serde(deserialize_with = "deserialize_bounded_token")] String);

fn deserialize_bounded_token<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let token = String::deserialize(deserializer)?;
    if token.len() > 16 * 1024 {
        return Err(serde::de::Error::custom(
            "opaque execution token exceeds size limit",
        ));
    }
    Ok(token)
}

impl std::fmt::Debug for OpaqueEventCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpaqueEventCursor([REDACTED])")
    }
}

pub struct InferenceExecution {
    pub events: InferenceEventStream,
    pub handle: Option<OpaqueExecutionHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferenceStatus {
    Running,
    Completed,
    Cancelled,
}

#[async_trait]
pub trait InferenceGateway: Send + Sync {
    async fn describe(&self, model: &ModelRef) -> Result<ModelDescriptor, InferenceError> {
        Err(InferenceError::new(
            ErrorClass::UnsupportedCapability,
            format!("{}/{}", model.provider, model.model),
            "describe",
            "model capability discovery is not supported",
        ))
    }

    async fn start(
        &self,
        request: InferenceRequest,
        cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError>;

    async fn attach(
        &self,
        _handle: OpaqueExecutionHandle,
        _after: Option<OpaqueEventCursor>,
        _cancel: CancellationToken,
    ) -> Result<InferenceEventStream, InferenceError> {
        Err(InferenceError::new(
            ErrorClass::UnsupportedCapability,
            "gateway",
            "attach",
            "durable execution is not supported",
        ))
    }

    async fn cancel(
        &self,
        _handle: OpaqueExecutionHandle,
    ) -> Result<InferenceStatus, InferenceError> {
        Err(InferenceError::new(
            ErrorClass::UnsupportedCapability,
            "gateway",
            "cancel",
            "durable execution is not supported",
        ))
    }
}
