//! Error paths and gateway failure recovery.

use std::sync::Arc;

use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::FauxProvider;
use crate::runtime::{test_agent_spec, test_config, test_supervisor, wait_for_task_status};

#[tokio::test]
async fn test_run_on_unregistered_agent() {
    let config = test_config();
    let faux: Arc<dyn llmd::gateway::LlmGateway> = Arc::new(FauxProvider::new());
    let core = test_supervisor(faux, config).await;

    let _result = core
        .run(
            "hello",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("ghost".to_string()),
                },
                history: None,
                host_context: None,
                ..Default::default()
            }),
        )
        .await;

    let snapshot = core.snapshot().await;
    assert!(snapshot.agents.contains_key("ghost"));
}

#[tokio::test]
async fn test_run_with_model_error() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_error("API overloaded").await;

    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let spec = test_agent_spec("error-agent");
    core.register_agent(spec).await;

    let result = core
        .run(
            "test",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("error-agent".to_string()),
                },
                history: None,
                host_context: None,
                ..Default::default()
            }),
        )
        .await;

    assert!(result.total_steps <= 5);
}

#[tokio::test]
async fn test_reused_root_task_recovers_after_gateway_failure() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_error("temporary failure").await;
    faux.push_text("recovered response").await;
    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("recovering-root"))
        .await;

    let options = |source_turn_id: &str, work_id: &str| {
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("recovering-root".into()),
            },
            history: None,
            host_context: Some(piko_protocol::agents::HostTaskContext::new(
                "session_recovering_root",
            )),
            source_turn_id: Some(source_turn_id.into()),
            work_id: Some(work_id.into()),
        })
    };

    let first = core.run("first", options("turn_1", "work_1")).await;
    assert_eq!(first.status, piko_protocol::runtime::RunStatus::Completed);
    let first_snapshot = core.snapshot().await;
    let task_id = first_snapshot
        .tasks
        .keys()
        .next()
        .expect("root task registered")
        .clone();

    let failed = core.run("fail once", options("turn_2", "work_2")).await;
    assert_eq!(failed.status, piko_protocol::runtime::RunStatus::Error);
    let failed_snapshot = core.snapshot().await;
    assert_eq!(
        failed_snapshot.tasks.len(),
        1,
        "unexpected root tasks: {:?}",
        failed_snapshot
            .tasks
            .iter()
            .map(|(id, task)| (id, &task.status))
            .collect::<Vec<_>>()
    );
    wait_for_task_status(
        &core,
        &task_id,
        piko_protocol::agents::AgentTaskStatus::Failed,
    )
    .await;

    let recovered = core.run("try again", options("turn_3", "work_3")).await;
    assert_eq!(
        recovered.status,
        piko_protocol::runtime::RunStatus::Completed
    );
    assert!(recovered.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "recovered response"
            ))
    )));
    let recovered_snapshot = core.snapshot().await;
    assert_eq!(recovered_snapshot.tasks.len(), 1);
    assert!(matches!(
        recovered_snapshot
            .tasks
            .get(&task_id)
            .map(|task| &task.status),
        Some(piko_protocol::agents::AgentTaskStatus::Idle)
    ));
}
