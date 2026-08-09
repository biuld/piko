mod support;
pub use support::{MockSessionPublisher, successful_turn_run};

#[path = "support/mock_turn_runner.rs"]
mod mock_turn_runner;

use std::sync::Arc;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::api::{Command, ServerMessage as Event, SessionTreeEntry};
use piko_hostd::domain::sessions::SessionModelRef;
use piko_hostd::infra::storage::JsonlSessionRepository;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use piko_protocol::PromptResourceSnapshot;

#[derive(Clone)]
struct CapturingRunner(Arc<std::sync::Mutex<Vec<PromptResourceSnapshot>>>);

#[async_trait]
impl AgentRunRunner for CapturingRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        if let Some(resources) = &input.prompt_resources {
            self.0.lock().unwrap().push(resources.clone());
        }
        MockAgentRunRunner.run_agent(input).await
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
        Arc::new(CapturingRunner(captured.clone())),
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
        .handle_command(Command::ChatSubmit {
            command_id: "s1".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent.clone(),
            text: "hello".into(),
        })
        .await;
    server
        .handle_command(Command::ChatSubmit {
            command_id: "s2".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent,
            text: "again".into(),
        })
        .await;

    let snapshots = captured.lock().unwrap().clone();
    assert_eq!(snapshots.len(), 2);

    // First run: full snapshot with identity facts and run_kind: initial.
    let first = world_state(&snapshots[0]);
    assert!(first.contains("session_id:"));
    assert!(first.contains("agent_instance_id:"));
    assert!(first.contains("operation_id: turn_"));
    assert!(first.contains("run_kind: initial"));
    assert!(first.contains("model: model-a"));
    assert!(!first.contains("world-state changed"));

    // Second run: diff only — changed operation id and run_kind, no repeated
    // identity facts, no model line (unchanged).
    let second = world_state(&snapshots[1]);
    assert!(second.starts_with("world-state changed since the previous run:"));
    assert!(second.contains("operation_id: turn_"));
    assert!(second.contains("run_kind: continuation"));
    assert!(!second.contains("session_id:"));
    assert!(!second.contains("agent_instance_id:"));
    assert!(!second.contains("model:"));

    // Durable baseline: the manifest keeps the last recorded facts, and a
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
            .is_some_and(|id| id.starts_with("turn_"))
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
        Arc::new(CapturingRunner(captured.clone())),
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
        .handle_command(Command::ChatSubmit {
            command_id: "s1".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent.clone(),
            text: "hello".into(),
        })
        .await;

    server
        .set_active_model(Some(SessionModelRef::new("anthropic", "model-b")))
        .await;
    let second_turn_events = server
        .handle_command(Command::ChatSubmit {
            command_id: "s2".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent,
            text: "switch".into(),
        })
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
        Arc::new(CapturingRunner(captured.clone())),
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
        .handle_command(Command::ChatSubmit {
            command_id: "s1".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent,
            text: "hello".into(),
        })
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
