use std::collections::BTreeSet;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::{StreamExt, stream};
use piko_llmd::capabilities::{ExecutionCapability, ModelCapabilities, ModelLimits};
use piko_llmd::gateway::{
    ErrorClass, FinishReason, InferenceError, InferenceEvent, InferenceEventStream,
    InferenceExecution, InferenceGateway, InferenceRequest, InferenceStatus, ModelDescriptor,
    ModelRef, OpaqueEventCursor, OpaqueExecutionHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct DurableFixture {
    cancelled: Mutex<bool>,
}

fn opaque_handle(value: &str) -> OpaqueExecutionHandle {
    serde_json::from_value(serde_json::json!(value)).unwrap()
}

fn opaque_cursor(value: &str) -> OpaqueEventCursor {
    serde_json::from_value(serde_json::json!(value)).unwrap()
}

fn events(after_first: bool) -> InferenceEventStream {
    let mut events = Vec::new();
    if !after_first {
        events.extend([
            InferenceEvent::Cursor(opaque_cursor("cursor-1")),
            InferenceEvent::text("durable output"),
        ]);
    }
    events.extend([
        InferenceEvent::Cursor(opaque_cursor("cursor-2")),
        InferenceEvent::Completed(FinishReason::Completed {
            reason: "stop".into(),
        }),
    ]);
    Box::pin(stream::iter(events))
}

#[async_trait]
impl InferenceGateway for DurableFixture {
    async fn describe(&self, model: &ModelRef) -> Result<ModelDescriptor, InferenceError> {
        let capabilities = ModelCapabilities {
            execution: BTreeSet::from([
                ExecutionCapability::Foreground,
                ExecutionCapability::Durable,
                ExecutionCapability::ResumableStream,
                ExecutionCapability::Cancellation,
            ]),
            ..ModelCapabilities::default()
        };
        Ok(ModelDescriptor {
            model: model.clone(),
            display_name: "durable fixture".into(),
            capabilities,
            limits: ModelLimits::default(),
        })
    }

    async fn start(
        &self,
        _request: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        Ok(InferenceExecution {
            events: events(false),
            handle: Some(opaque_handle("execution-1")),
        })
    }

    async fn attach(
        &self,
        handle: OpaqueExecutionHandle,
        after: Option<OpaqueEventCursor>,
        _cancel: CancellationToken,
    ) -> Result<InferenceEventStream, InferenceError> {
        if handle != opaque_handle("execution-1") {
            return Err(InferenceError::new(
                ErrorClass::Target,
                "fixture",
                "attach",
                "unknown execution handle",
            ));
        }
        Ok(events(after.as_ref() == Some(&opaque_cursor("cursor-1"))))
    }

    async fn cancel(
        &self,
        handle: OpaqueExecutionHandle,
    ) -> Result<InferenceStatus, InferenceError> {
        if handle != opaque_handle("execution-1") {
            return Err(InferenceError::new(
                ErrorClass::Target,
                "fixture",
                "cancel",
                "unknown execution handle",
            ));
        }
        *self.cancelled.lock().unwrap() = true;
        Ok(InferenceStatus::Cancelled)
    }
}

fn request() -> InferenceRequest {
    InferenceRequest::text_task(
        ModelRef::new("fixture", "durable"),
        "be concise",
        vec![piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String("run".into()),
            timestamp: None,
        }],
        piko_llmd::gateway::InvocationContext {
            session_id: "session".into(),
            agent_instance_id: "root".into(),
            run_id: "run".into(),
            step_id: "step".into(),
            step_message_id: "step-message".into(),
        },
    )
}

#[tokio::test]
async fn detach_restore_resume_deduplicate_and_cancel_contract() {
    let gateway = DurableFixture::default();
    let descriptor = gateway
        .describe(&ModelRef::new("fixture", "durable"))
        .await
        .unwrap();
    assert!(
        descriptor
            .capabilities
            .execution
            .contains(&ExecutionCapability::Durable)
    );

    let execution = gateway
        .start(request(), CancellationToken::new())
        .await
        .unwrap();
    let handle = execution.handle.unwrap();
    drop(execution.events);

    let persisted = serde_json::to_string(&handle).unwrap();
    let restored: OpaqueExecutionHandle = serde_json::from_str(&persisted).unwrap();
    assert_eq!(restored, handle);
    assert!(!format!("{restored:?}").contains("execution-1"));

    let full = gateway
        .attach(restored.clone(), None, CancellationToken::new())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    assert_eq!(
        full.iter()
            .filter(|event| matches!(event, InferenceEvent::TextDelta { .. }))
            .count(),
        1
    );

    let resumed = gateway
        .attach(
            restored.clone(),
            Some(opaque_cursor("cursor-1")),
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    assert!(
        resumed
            .iter()
            .all(|event| !matches!(event, InferenceEvent::TextDelta { .. }))
    );
    assert!(matches!(resumed.last(), Some(InferenceEvent::Completed(_))));
    assert_eq!(
        gateway.cancel(restored).await.unwrap(),
        InferenceStatus::Cancelled
    );
}
