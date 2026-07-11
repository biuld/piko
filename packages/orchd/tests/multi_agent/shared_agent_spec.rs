//! Multiple task instances sharing the same AgentSpec.

use std::sync::Arc;

use piko_protocol::agents::HostTaskContext;
use piko_protocol::runtime::{OrchRunCommandOptions, OrchRunOptions};

use crate::faux_provider::FauxProvider;
use crate::runtime::{test_agent_spec, test_config, test_supervisor};

#[tokio::test]
async fn test_sequential_tasks_on_same_agent() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("first response").await;
    faux.push_text("second response").await;

    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    let spec = test_agent_spec("worker");
    core.register_agent(spec).await;

    let r1 = core
        .run(
            "task1",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("worker".into()),
                },
                history: None,
                host_context: None,
                ..Default::default()
            }),
        )
        .await;
    assert_eq!(r1.status, piko_protocol::runtime::RunStatus::Completed);

    let r2 = core
        .run(
            "task2",
            Some(OrchRunOptions {
                command: OrchRunCommandOptions {
                    target_agent_id: Some("worker".into()),
                },
                history: None,
                host_context: None,
                ..Default::default()
            }),
        )
        .await;
    assert_eq!(r2.status, piko_protocol::runtime::RunStatus::Completed);
}

#[tokio::test]
async fn test_root_task_reuse_is_scoped_by_session() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("session a first").await;
    faux.push_text("session b first").await;
    faux.push_text("session a second").await;
    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;
    core.register_agent(test_agent_spec("shared-agent")).await;

    let options = |session_id: &str, source_turn_id: &str| {
        Some(OrchRunOptions {
            command: OrchRunCommandOptions {
                target_agent_id: Some("shared-agent".into()),
            },
            history: None,
            host_context: Some(HostTaskContext::new(session_id)),
            source_turn_id: Some(source_turn_id.into()),
            work_id: Some(format!("{source_turn_id}_work")),
        })
    };

    core.run("a1", options("session_a", "turn_a1")).await;
    core.run("b1", options("session_b", "turn_b1")).await;
    let second_a = core.run("a2", options("session_a", "turn_a2")).await;

    assert!(second_a.messages.iter().any(|message| matches!(
        message,
        piko_protocol::Message::Assistant { content, .. }
            if content.iter().any(|block| matches!(
                block,
                piko_protocol::ContentBlock::Text { text } if text == "session a second"
            ))
    )));
    assert_eq!(core.snapshot().await.tasks.len(), 2);
}

#[tokio::test]
async fn test_multiple_agents_concurrent() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("a1 response").await;
    faux.push_text("a2 response").await;

    let config = test_config();
    let core = test_supervisor(faux as Arc<dyn llmd::gateway::LlmGateway>, config).await;

    core.register_agent(test_agent_spec("a1")).await;
    core.register_agent(test_agent_spec("a2")).await;

    let core1 = core.clone();
    let core2 = core.clone();

    let h1 = tokio::spawn(async move {
        core1
            .run(
                "task1",
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some("a1".into()),
                    },
                    history: None,
                    host_context: None,
                    ..Default::default()
                }),
            )
            .await
    });

    let h2 = tokio::spawn(async move {
        core2
            .run(
                "task2",
                Some(OrchRunOptions {
                    command: OrchRunCommandOptions {
                        target_agent_id: Some("a2".into()),
                    },
                    history: None,
                    host_context: None,
                    ..Default::default()
                }),
            )
            .await
    });

    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();

    assert_eq!(r1.status, piko_protocol::runtime::RunStatus::Completed);
    assert_eq!(r2.status, piko_protocol::runtime::RunStatus::Completed);
}
