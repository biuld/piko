use std::sync::Arc;

use orchd::AgentRuntimeService;
use orchd::host::Supervisor;
use orchd::integration::PersistSink;
use orchd::testing::CollectingPersistSink;
use piko_protocol::MessageContent;
use piko_protocol::agent_runtime::{
    CreateTaskRequest, InputDelivery, InputSource, SubmitTaskInput, TaskMode,
};
use piko_protocol::agents::{AgentSpec, HostTaskContext};
use piko_protocol::config::OrchdConfig;

use crate::faux_provider::FauxProvider;

pub fn test_config() -> OrchdConfig {
    let mut config = OrchdConfig::single_provider("faux", "test-key", "faux-1");
    config.agents.clear();
    config
}

pub fn test_agent_spec(id: &str) -> AgentSpec {
    AgentSpec {
        id: id.to_string(),
        name: id.to_string(),
        role: "test".to_string(),
        description: None,
        system_prompt: "You are a test agent.".to_string(),
        model: None,
        tool_set_ids: vec![],
        active_tool_names: None,
        thinking_level: None,
    }
}

pub async fn setup_runtime() -> (Arc<Supervisor>, AgentRuntimeService) {
    let faux = Arc::new(FauxProvider::new());
    faux.push_text("idempotent response").await;
    let core =
        Supervisor::from_config(faux as Arc<dyn llmd::gateway::LlmGateway>, test_config()).await;
    core.set_persist_sink(Arc::new(CollectingPersistSink::new()) as Arc<dyn PersistSink>)
        .await;
    core.register_agent(test_agent_spec("idem")).await;
    let runtime = AgentRuntimeService::new(Arc::clone(&core));
    (core, runtime)
}

pub fn sample_create_request() -> CreateTaskRequest {
    CreateTaskRequest {
        request_id: "req-create-1".into(),
        session_id: "session-idem".into(),
        task_id: Some("task_idem_root".into()),
        agent_id: "idem".into(),
        parent_task_id: None,
        source: InputSource::User,
        mode: TaskMode::Attached,
        host_context: HostTaskContext::new("session-idem"),
        resume: None,
    }
}

pub fn sample_submit_input(task_id: &str) -> SubmitTaskInput {
    SubmitTaskInput {
        request_id: "req-input-1".into(),
        session_id: "session-idem".into(),
        task_id: task_id.to_string(),
        message_id: "msg-input-1".into(),
        work_id: "work-idem-1".into(),
        source_turn_id: None,
        source: InputSource::User,
        content: MessageContent::String("hello".into()),
        delivery: InputDelivery::AfterCurrentStep,
        submitted_at: 1,
    }
}
