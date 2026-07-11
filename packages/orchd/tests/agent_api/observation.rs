use futures_util::StreamExt;
use orchd::api::AgentRuntime;
use piko_protocol::agent_runtime::{SessionCursor, SubscribeRequest};

use super::support::{sample_create_request, sample_submit_input, setup_runtime};

#[tokio::test]
async fn session_hub_receives_task_changed_on_create() {
    let (_core, runtime) = setup_runtime().await;
    let handle = runtime.create_task(sample_create_request()).await.unwrap();

    let subscription = runtime
        .subscribe_session(SubscribeRequest {
            session_id: "session-idem".into(),
            task_id: None,
            after: None,
        })
        .await
        .unwrap();
    let mut output = subscription.output;

    let _ = runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut saw_task_changed = false;
    while tokio::time::Instant::now() < deadline {
        if let Ok(Some(Ok(envelope))) =
            tokio::time::timeout(std::time::Duration::from_millis(200), output.next()).await
            && let piko_protocol::agent_runtime::SessionOutput::Event(event) = envelope.output
            && matches!(
                event.event,
                piko_protocol::agent_runtime::SessionEvent::TaskChanged { .. }
            )
        {
            saw_task_changed = true;
            break;
        }
    }
    assert!(saw_task_changed, "expected TaskChanged on session hub");
}

#[tokio::test]
async fn invalid_subscription_cursor_is_reported_on_stream() {
    let (_core, runtime) = setup_runtime().await;
    let subscription = runtime
        .subscribe_session(SubscribeRequest {
            session_id: "session-idem".into(),
            task_id: None,
            after: Some(SessionCursor {
                epoch: "stale-epoch".into(),
                seq: 0,
            }),
        })
        .await
        .unwrap();
    let mut output = subscription.output;
    assert!(matches!(
        output.next().await,
        Some(Err(orchd::api::SessionStreamError::SnapshotRequired {
            reason: orchd::api::SnapshotRequiredReason::EpochChanged,
        }))
    ));
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn session_snapshot_excludes_tasks_from_other_sessions() {
    let (_core, runtime) = setup_runtime().await;
    runtime.create_task(sample_create_request()).await.unwrap();
    let mut other = sample_create_request();
    other.request_id = "req-create-other".into();
    other.session_id = "session-other".into();
    other.task_id = Some("task-other".into());
    other.host_context = piko_protocol::agents::HostTaskContext::new("session-other");
    runtime.create_task(other).await.unwrap();

    let snapshot = runtime
        .session_snapshot("session-idem".into())
        .await
        .unwrap();
    assert_eq!(snapshot.tasks.len(), 1);
    assert_eq!(snapshot.tasks[0].task_id, "task_idem_root");
}
