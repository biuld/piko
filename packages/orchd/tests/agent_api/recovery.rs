use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::api::AgentRuntime;
use orchd::testing::CollectingPersistSink;
use orchd::testing::Supervisor;
use orchd_api::PersistSink;
use piko_protocol::MessageContent;

use super::support::{sample_create_request, sample_submit_input, test_agent_spec, test_config};
use crate::faux_provider::FauxProvider;

#[tokio::test]
async fn resumed_task_continues_persisted_sequence_without_recreating_task() {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("resumed response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    let sink = Arc::new(CollectingPersistSink::new());
    core.set_persist_sink(sink.clone() as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(core);
    let mut request = sample_create_request();
    request.resume = Some(piko_protocol::agent_runtime::TaskResumeState {
        transcript: vec![piko_protocol::Message::User {
            content: MessageContent::String("old input".into()),
            timestamp: Some(1),
        }],
        head_message_id: Some("msg-old".into()),
        last_task_seq: 7,
        committed_message_ids: vec!["msg-old".into()],
    });
    let handle = runtime.create_task(request).await.unwrap();
    runtime
        .submit_input(sample_submit_input(&handle.task_id))
        .await
        .unwrap();

    let commit = sink
        .messages()
        .into_iter()
        .find(|commit| commit.message_id == "msg-input-1")
        .unwrap();
    assert_eq!(commit.task_seq, 8);
    assert_eq!(commit.parent_message_id.as_deref(), Some("msg-old"));
    assert!(
        sink.task_events()
            .into_iter()
            .all(|commit| { !matches!(commit.event, piko_protocol::TaskEvent::Created { .. }) })
    );
}
