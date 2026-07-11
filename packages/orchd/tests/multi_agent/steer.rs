//! Task close/reopen/steer control behavior.

use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::api::AgentRuntime;
use piko_protocol::ServerMessage as Event;
use piko_protocol::agent_runtime::TaskControlRequest;
use piko_protocol::agents::HostTaskContext;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::FauxProvider;
use crate::runtime::{
    TEST_STREAM_TIMEOUT, run_test_stream, test_agent_spec, test_config, test_supervisor,
    wait_for_task_status,
};
use crate::session_output::collect_test_events;

#[tokio::test]
async fn test_task_control_close_reopen_and_steer() {
    let config = test_config();
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_text("second response").await;
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("controlled")).await;

    let stream = run_test_stream(
        &core,
        "hello",
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("controlled".to_string()),
            },
            history: None,
            host_context: Some(HostTaskContext::new("session_control".to_string())),
            ..Default::default()
        }),
    )
    .await;

    let events = collect_test_events(stream, TEST_STREAM_TIMEOUT).await;

    let mut task_id = None;
    for event in &events {
        match event {
            Event::TaskLifecycle(piko_protocol::TaskEvent::Created {
                task_id: created_task_id,
                ..
            }) => {
                task_id = Some(created_task_id.clone());
            }
            Event::TaskLifecycle(piko_protocol::TaskEvent::Idle { .. }) => break,
            _ => {}
        }
    }

    let task_id = task_id.expect("expected task id");
    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    runtime
        .control_task(TaskControlRequest::Close {
            request_id: "req-close-task".into(),
            task_id: task_id.clone(),
        })
        .await
        .unwrap();
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Closed,
    )
    .await;
    let snapshot = core.snapshot().await;
    let closed_status = snapshot.tasks.get(&task_id).map(|task| task.status.clone());
    assert!(
        matches!(
            closed_status,
            Some(piko_protocol::agents::AgentTaskStatus::Closed)
        ),
        "expected Closed, got {closed_status:?}"
    );

    runtime
        .control_task(TaskControlRequest::Reopen {
            request_id: "req-reopen-task".into(),
            task_id: task_id.clone(),
        })
        .await
        .unwrap();
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Idle,
    )
    .await;
    let snapshot = core.snapshot().await;
    let reopened_status = snapshot.tasks.get(&task_id).map(|task| task.status.clone());
    assert!(
        matches!(
            reopened_status,
            Some(piko_protocol::agents::AgentTaskStatus::Idle)
        ),
        "expected Idle, got {reopened_status:?}"
    );

    let result = core
        .run(
            "resume",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("controlled".to_string()),
                },
                history: None,
                host_context: Some(HostTaskContext::new("session_control".to_string())),
                ..Default::default()
            }),
        )
        .await;

    assert!(result.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "second response"
            ))
    )));
}
