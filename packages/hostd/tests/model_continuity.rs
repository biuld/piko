mod support;
pub use support::MockSessionPublisher;

#[path = "support/mock_agent_runner.rs"]
mod mock_agent_runner;

use std::sync::Arc;

use async_trait::async_trait;
use mock_agent_runner::MockAgentRunRunner;
use piko_hostd::api::{Command, ServerMessage as Event, SessionTreeEntry};
use piko_hostd::domain::sessions::SessionModelRef;
use piko_hostd::infra::storage::JsonlSessionRepository;
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::HostServer;
use piko_protocol::PromptResourceSnapshot;

#[derive(Clone)]
struct CapturingRunner(
    Arc<std::sync::Mutex<Vec<PromptResourceSnapshot>>>,
    MockAgentRunRunner,
);

#[async_trait]
impl AgentRunRunner for CapturingRunner {
    async fn ensure_session_runtime(
        &self,
        session_id: &str,
        cwd: &str,
        session_dir: &std::path::Path,
        resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        AgentRunRunner::ensure_session_runtime(&self.1, session_id, cwd, session_dir, resume_agent)
            .await
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        if let Some(resources) = &runtime.prompt_resources {
            self.0.lock().unwrap().push(resources.clone());
        }
        AgentRunRunner::submit_agent_input(&self.1, input, runtime).await
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
        disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        AgentRunRunner::wait_agent_input_started(
            &self.1,
            session_id,
            agent_instance_id,
            input_id,
            disposition,
        )
        .await
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        AgentRunRunner::wait_agent_input_completion(
            &self.1,
            session_id,
            agent_instance_id,
            input_id,
        )
        .await
    }

    async fn cancel_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, piko_hostd::api::ProtocolError> {
        AgentRunRunner::cancel_agent_input(&self.1, session_id, agent_instance_id, input_id).await
    }

    async fn interrupt_agent(&self, session_id: &str, agent_instance_id: &str) -> bool {
        AgentRunRunner::interrupt_agent(&self.1, session_id, agent_instance_id).await
    }

    async fn list_agent_instances(
        &self,
        session_id: &str,
    ) -> Option<Vec<piko_protocol::AgentInfo>> {
        AgentRunRunner::list_agent_instances(&self.1, session_id).await
    }

    async fn recover_observation(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            piko_orchd_api::SessionSubscription,
        ),
        piko_hostd::api::ProtocolError,
    > {
        AgentRunRunner::recover_observation(&self.1, session_id, agent_instance_id, input_id).await
    }

    async fn finish_agent_run(&self, session_id: &str, agent_instance_id: &str, input_id: &str) {
        AgentRunRunner::finish_agent_run(&self.1, session_id, agent_instance_id, input_id).await;
    }

    async fn has_active_session_run(&self, session_id: &str) -> bool {
        AgentRunRunner::has_active_session_run(&self.1, session_id).await
    }
}

fn created_session_id(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created")
}

fn world_state(snapshot: &PromptResourceSnapshot) -> String {
    let Some(message) = &snapshot.world_state else {
        return String::new();
    };
    match message {
        piko_protocol::messages::Message::Context {
            content: piko_protocol::messages::MessageContent::String(text),
            ..
        } => text.clone(),
        _ => String::new(),
    }
}

#[tokio::test]
async fn world_state_is_injected_full_then_diff_and_baseline_is_durable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(root),
        Arc::new(CapturingRunner(
            captured.clone(),
            MockAgentRunRunner::default(),
        )),
    );

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created_session_id(&created);
    let root_agent = format!("agent_{session_id}_root");

    server
        .set_active_model(Some(SessionModelRef::new("openai", "model-a")))
        .await;
    server
        .handle_command(Command::submit_follow_up(
            "s1",
            session_id.clone(),
            root_agent.clone(),
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;
    server
        .handle_command(Command::submit_follow_up(
            "s2",
            session_id.clone(),
            root_agent,
            piko_protocol::MessageContent::String("again".into()),
        ))
        .await;

    let snapshots = captured.lock().unwrap().clone();
    assert_eq!(snapshots.len(), 2);

    // First run: full snapshot with identity facts and run_kind: initial.
    let first = world_state(&snapshots[0]);
    assert!(first.contains("session_id:"));
    assert!(first.contains("agent_instance_id:"));
    assert!(first.contains("operation_id: input_s1"));
    assert!(first.contains("run_kind: initial"));
    assert!(first.contains("model: model-a"));
    assert!(!first.contains("world-state changed"));

    // Second run: diff only — changed operation id and run_kind, no repeated
    // identity facts, no model line (unchanged).
    let second = world_state(&snapshots[1]);
    assert!(second.starts_with("world-state changed since the previous run:"));
    assert!(second.contains("operation_id: input_s2"));
    assert!(second.contains("run_kind: continuation"));
    assert!(!second.contains("session_id:"));
    assert!(!second.contains("agent_instance_id:"));
    assert!(!second.contains("model:"));

    // Durable baseline: the journal projection keeps the last recorded facts, and a
    // fresh host reloading the same directory restores them.
    let repo = JsonlSessionRepository::new(root);
    let loaded = repo.list(None).expect("list sessions");
    let persisted = loaded
        .iter()
        .find(|session| session.state.session_id == session_id)
        .expect("persisted session");
    let baseline = persisted
        .state
        .world_state_baseline
        .as_ref()
        .expect("world-state baseline persisted");
    assert_eq!(
        baseline.run_kind,
        piko_hostd::domain::prompts::RunKind::Continuation
    );
    assert!(
        baseline
            .operation_id
            .as_deref()
            .is_some_and(|id| id == "input_s2")
    );

    let reopened = repo
        .load_by_path(&persisted.path)
        .expect("reload session from disk");
    assert_eq!(
        reopened.state.world_state_baseline,
        persisted.state.world_state_baseline
    );
}

#[tokio::test]
async fn session_model_continuity_is_durable_and_drives_prompt_fragment() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(root),
        Arc::new(CapturingRunner(
            captured.clone(),
            MockAgentRunRunner::default(),
        )),
    );

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created_session_id(&created);
    let root_agent = format!("agent_{session_id}_root");

    server
        .set_active_model(Some(SessionModelRef::new("openai", "model-a")))
        .await;
    server
        .handle_command(Command::submit_follow_up(
            "s1",
            session_id.clone(),
            root_agent.clone(),
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;

    server
        .set_active_model(Some(SessionModelRef::new("anthropic", "model-b")))
        .await;
    let second_turn_events = server
        .handle_command(Command::submit_follow_up(
            "s2",
            session_id.clone(),
            root_agent,
            piko_protocol::MessageContent::String("switch".into()),
        ))
        .await;

    let snapshots = captured.lock().unwrap().clone();
    assert_eq!(snapshots.len(), 2, "one frozen prompt per turn");

    // First run: world-state shows the resolved model, no model-switch.
    assert!(world_state(&snapshots[0]).contains("model: model-a"));
    assert!(
        !snapshots[0]
            .blocks
            .iter()
            .any(|block| block.id == "context.model-switch")
    );

    // Second run after the model change: one model-switch fragment naming both.
    let switch = snapshots[1]
        .blocks
        .iter()
        .find(|block| block.id == "context.model-switch")
        .expect("model-switch fragment on model change");
    assert!(switch.content.contains("\"model-a\""));
    assert!(switch.content.contains("\"model-b\""));

    assert!(
        second_turn_events.iter().any(|event| matches!(
            event,
            Event::SessionEntryCommitted(committed)
                if committed.session_id == session_id
                    && matches!(
                        &committed.entry,
                        SessionTreeEntry::ModelChange(change)
                            if change.provider == "anthropic" && change.model_id == "model-b"
                    )
        )),
        "model change is projected live after its durable append"
    );

    // Durable record + exactly one JSONL ModelChange marker, reloaded from disk.
    let repo = JsonlSessionRepository::new(root);
    let loaded = repo.list(None).expect("list sessions");
    let persisted = loaded
        .iter()
        .find(|session| session.state.session_id == session_id)
        .expect("persisted session");
    assert_eq!(
        persisted
            .state
            .last_model
            .as_ref()
            .map(|model| (model.provider.as_str(), model.model_id.as_str())),
        Some(("anthropic", "model-b"))
    );
    let markers = persisted
        .state
        .entries
        .iter()
        .filter(|entry| matches!(entry, SessionTreeEntry::ModelChange(_)))
        .count();
    assert_eq!(markers, 1);

    // A fresh host loading the same directory preserves continuity.
    let reopened = repo
        .load_by_path(&persisted.path)
        .expect("reload session from disk");
    assert_eq!(
        reopened
            .state
            .last_model
            .as_ref()
            .map(|model| model.model_id.as_str()),
        Some("model-b")
    );
}

#[tokio::test]
async fn unconfigured_active_model_produces_no_model_fragments() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let captured = Arc::new(std::sync::Mutex::new(Vec::new()));
    let server = HostServer::with_storage_runner_settings(
        JsonlSessionRepository::new(root),
        Arc::new(CapturingRunner(
            captured.clone(),
            MockAgentRunRunner::default(),
        )),
        piko_hostd::domain::config::HostSettings::default(),
    );

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created_session_id(&created);
    let root_agent = format!("agent_{session_id}_root");

    // Without auth/registry, rebuilding the runner leaves the active model
    // unset; a submitted turn must not fabricate a model or a switch.
    server
        .handle_command(Command::submit_follow_up(
            "s1",
            session_id.clone(),
            root_agent,
            piko_protocol::MessageContent::String("hello".into()),
        ))
        .await;
    let snapshots = captured.lock().unwrap().clone();
    assert_eq!(snapshots.len(), 1);
    assert!(
        !snapshots[0]
            .blocks
            .iter()
            .any(|block| block.id == "context.model-switch")
    );
    assert!(!world_state(&snapshots[0]).contains("model:"));
}
