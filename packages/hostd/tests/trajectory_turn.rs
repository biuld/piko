//! End-to-end trajectory verification (F-36): a real turn through the hostd
//! path writes assembly, model-step, and tool-call records to the session
//! journal as optional events, and the trajectory query replays them.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use piko_hostd::adapters::OrchAgentRunRunner;
use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::application::TrajectoryQuery;
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use piko_protocol::{TRAJECTORY_EVENT_ASSEMBLY, TRAJECTORY_EVENT_TOOL_CALL};
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

fn execution(events: Vec<InferenceEvent>) -> InferenceExecution {
    InferenceExecution {
        events: Box::pin(iter(events)),
        handle: None,
    }
}

/// Step 1 emits a tool call for `todo_write`; later steps reply in text so
/// the run terminates.
struct ScriptedGateway {
    step: AtomicUsize,
}

impl ScriptedGateway {
    fn new() -> Self {
        Self {
            step: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl InferenceGateway for ScriptedGateway {
    async fn start(
        &self,
        _request: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        if step == 0 {
            Ok(execution(vec![
                InferenceEvent::function_call(
                    "call-todo",
                    "todo_write",
                    r#"{"todos":[{"id":1,"status":"pending","content":"plan"}]}"#,
                ),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("tool_use"),
            ]))
        } else {
            Ok(execution(vec![
                InferenceEvent::text("done"),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("stop"),
            ]))
        }
    }
}

async fn wait_for_events(
    store: &SessionStore,
    expected: usize,
    event_type: &str,
) -> Vec<piko_session_store::RawJournalEvent> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let events = store.raw_journal_events().unwrap_or_default();
        let matched = events
            .iter()
            .filter(|event| event.event.event_type == event_type)
            .count();
        if matched >= expected {
            return events;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "trajectory {event_type} records not appended (got {matched}/{expected})"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn turn_writes_durable_trajectory_records() {
    let temp = tempfile::tempdir().unwrap();
    let initial = HostServer::with_storage(JsonlSessionRepository::new(temp.path()));
    let created = initial
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap();
    let listed = initial
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let session_path = listed
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionListed { sessions, .. }),
                ..
            } => sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .and_then(|session| session.session_path.clone()),
            _ => None,
        })
        .unwrap();
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    let root_agent_instance_id = root.agent_instance_id.clone();

    let runner = Arc::new(
        OrchAgentRunRunner::new(Arc::new(ScriptedGateway::new()), "test", "test-model").await,
    );
    let server =
        HostServer::with_storage_and_runner(JsonlSessionRepository::new(temp.path()), runner);
    server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path.clone()),
        })
        .await;
    server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id: session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            after_seq: None,
        })
        .await;
    let events = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        server.handle_command(Command::ChatSubmit {
            command_id: "trajectory-turn".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent_instance_id.clone(),
            text: "plan this".into(),
        }),
    )
    .await
    .expect("turn should complete");
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::TurnLifecycle(piko_protocol::TurnEvent::Completed {
            agent_instance_id,
            ..
        }) if agent_instance_id == &root_agent_instance_id
    )));

    // Durable optional events landed in the session journal. Model-step
    // records are covered at the llmd layer (`captures_actual_model_step_*`);
    // the scripted gateway here replaces the real model executor.
    let events = wait_for_events(&store, 1, TRAJECTORY_EVENT_ASSEMBLY).await;
    let tool_events = events
        .iter()
        .filter(|event| event.event.event_type == TRAJECTORY_EVENT_TOOL_CALL)
        .collect::<Vec<_>>();
    assert_eq!(tool_events.len(), 2, "started + completed tool records");

    // A fresh query (separate store instance, ≈ restart) replays the run.
    let query = TrajectoryQuery::new(
        Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::from([
            (
                session_id.clone(),
                std::path::PathBuf::from(session_path.clone()),
            ),
        ]))),
        Arc::new(piko_hostd::adapters::storage::FsSessionStoreFactory),
        None,
    );
    let page = query
        .list_runs(&session_id, None, None, 50, &Default::default())
        .await
        .unwrap();
    assert_eq!(page.runs.len(), 1, "one run recorded");
    assert_eq!(page.runs[0].tool_call_count, 2);
    assert_eq!(
        page.runs[0].terminal,
        Some(piko_protocol::TrajectoryTerminalKind::Completed)
    );
    let run = query
        .fetch_run(&session_id, &page.runs[0].run_id, &Default::default())
        .await
        .unwrap();
    assert!(run.assembly.is_some());
    assert_eq!(run.records.len(), 2, "two tool records");
    assert!(!run.messages.is_empty(), "committed messages replayed");
}
