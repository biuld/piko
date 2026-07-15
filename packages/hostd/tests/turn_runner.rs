#[path = "support/mock_turn_runner.rs"]
mod mock_turn_runner;
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use hostd::protocol::HostServer;
use mock_turn_runner::MockTurnRunner;
use orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::SessionRuntimeSnapshot;
use support::{
    MockSessionPublisher, execution_running, execution_succeeded, success_report,
    successful_turn_run,
};
use tokio::sync::mpsc::unbounded_channel;
use tokio_stream::StreamExt;

#[derive(Clone, Default)]
struct RecoveringTurnRunner {
    agent_instance_id: Arc<std::sync::Mutex<Option<String>>>,
    turn_id: Arc<std::sync::Mutex<Option<String>>>,
    completion_tx: Arc<
        std::sync::Mutex<Option<tokio::sync::oneshot::Sender<hostd::ports::TurnRunCompletion>>>,
    >,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

#[async_trait]
impl TurnRunner for RecoveringTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        *self.agent_instance_id.lock().unwrap() = Some(input.turn_id.clone());
        *self.turn_id.lock().unwrap() = Some(input.turn_id.clone());
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        self.publishers.lock().unwrap().push(publisher.clone());
        let (completion_tx, completion) = tokio::sync::oneshot::channel();
        *self.completion_tx.lock().unwrap() = Some(completion_tx);
        let publish_session_id = input.session_id.clone();
        let publish_turn_id = input.turn_id.clone();
        let publish_agent_instance_id = input.turn_id.clone();
        tokio::spawn(async move {
            publisher.publish(
                publish_agent_instance_id.clone(),
                "main",
                1,
                execution_running(
                    publish_session_id,
                    publish_turn_id,
                    publish_agent_instance_id,
                    "main",
                ),
            );
            publisher.require_snapshot(orchd_api::SnapshotRequiredReason::CursorExpired);
        });
        Ok(TurnRunHandle {
            session_id: input.session_id,
            turn_id: input.turn_id,
            observation: subscription,
            completion,
        })
    }

    async fn recover_observation(
        &self,
        session_id: &str,
    ) -> Result<(SessionRuntimeSnapshot, SessionSubscription), hostd::api::ProtocolError> {
        let agent_instance_id = self.agent_instance_id.lock().unwrap().clone().unwrap();
        let (publisher, subscription) = MockSessionPublisher::new(session_id.to_string());
        self.publishers.lock().unwrap().push(publisher.clone());
        let cursor = subscription.cursor.clone();
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: cursor.epoch.clone(),
            seq: 0,
        };
        let recovered_session_id = session_id.to_string();
        let recovered_agent_instance_id = agent_instance_id.clone();
        let completion_tx = self.completion_tx.lock().unwrap().take();
        let completion_turn_id = self.turn_id.lock().unwrap().clone().unwrap();
        tokio::spawn(async move {
            publisher.publish(
                recovered_agent_instance_id.clone(),
                "main",
                2,
                execution_succeeded(
                    recovered_session_id.clone(),
                    recovered_agent_instance_id.clone(),
                    recovered_agent_instance_id,
                    "main",
                ),
            );
            if let Some(completion_tx) = completion_tx {
                let _ = completion_tx.send(hostd::ports::TurnRunCompletion {
                    session_id: recovered_session_id,
                    turn_id: completion_turn_id,
                    root_agent_instance_id: "root".into(),
                    result: Ok(success_report("root")),
                    observation_barrier: barrier,
                });
            }
        });
        Ok((
            SessionRuntimeSnapshot {
                session_id: session_id.to_string(),
                root_agent_instance_id: Some(agent_instance_id.clone()),
                active_agent_instance_id: Some(agent_instance_id),
                cursor,
            },
            subscription,
        ))
    }

    async fn pending_prompts_for_session(
        &self,
        _session_id: &str,
    ) -> (
        Vec<hostd::api::ApprovalSnapshot>,
        Vec<hostd::api::UserInteractionSnapshot>,
    ) {
        (
            vec![hostd::api::ApprovalSnapshot {
                approval_id: "approval-recovered".into(),
                tool_name: "bash".into(),
                request: serde_json::json!({"cmd": "pwd"}),
                status: hostd::api::ApprovalStatus::Pending,
            }],
            Vec::new(),
        )
    }
}

#[tokio::test]
async fn mock_turn_runner_completes_turn() {
    let runner = MockTurnRunner;
    let (ui_event_tx, _ui_event_rx) = unbounded_channel();
    let subscription = runner
        .run_turn(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            ui_event_tx,
            resume_root_agent: None,
        })
        .await
        .unwrap();

    let mut output = subscription.observation.output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn mock_turn_with_storage_populates_state() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockTurnRunner));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("unexpected {other:?}"),
    };

    let turn_events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;
    for event in &turn_events {
        if let Event::CommandResponse {
            result: Err(err), ..
        } = event
        {
            panic!("turn failed: {err}");
        }
    }

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
    assert!(
        !snapshot.entries.is_empty(),
        "expected user message in snapshot, got {snapshot:?}"
    );
}

#[tokio::test]
async fn turn_runner_returns_streaming_events() {
    let runner = MockTurnRunner;

    let (ui_event_tx, _ui_event_rx) = unbounded_channel();
    let subscription = runner
        .run_turn(TurnRunInput {
            session_id: "session-test".into(),
            turn_id: "turn-test".into(),
            prompt: "hello".into(),
            system_prompt: "system prompt".into(),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            ui_event_tx,
            resume_root_agent: None,
        })
        .await
        .unwrap();

    let mut output = subscription.observation.output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn snapshot_required_reconciles_and_resubscribes_without_losing_turn() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(RecoveringTurnRunner::default()),
    );
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap();

    let events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id,
            text: "hello".into(),
        })
        .await;

    assert!(
        events.iter().any(|event| matches!(event,
            Event::SessionReconciled(reconciled)
                if reconciled.reason == piko_protocol::ReconcileReason::RetentionExhausted
                    && reconciled.snapshot.pending_approvals.len() == 1
                    && reconciled.snapshot.active_turn.as_ref().is_some_and(|turn|
                        turn.status == piko_protocol::TurnStatus::WaitingForApproval)
        )),
        "events={events:?}"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}

#[derive(Clone)]
struct GatedTurnRunner {
    released: Arc<(std::sync::Mutex<bool>, tokio::sync::Notify)>,
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

impl GatedTurnRunner {
    fn new() -> Self {
        Self {
            released: Arc::new((std::sync::Mutex::new(false), tokio::sync::Notify::new())),
            prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn release(&self) {
        *self.released.0.lock().unwrap() = true;
        self.released.1.notify_waiters();
    }

    async fn wait_until_released(&self) {
        loop {
            if *self.released.0.lock().unwrap() {
                return;
            }
            self.released.1.notified().await;
        }
    }
}

#[async_trait]
impl TurnRunner for GatedTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        self.prompts.lock().unwrap().push(input.prompt.clone());
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let runner = self.clone();
        let session_id = input.session_id.clone();
        let source_turn_id = input.turn_id.clone();
        let agent_instance_id = input.turn_id.clone();
        tokio::spawn(async move {
            runner.wait_until_released().await;
            publisher.publish(
                agent_instance_id.clone(),
                "main",
                1,
                execution_running(
                    session_id.clone(),
                    source_turn_id.clone(),
                    agent_instance_id.clone(),
                    "main",
                ),
            );
            publisher.publish(
                agent_instance_id.clone(),
                "main",
                2,
                execution_succeeded(session_id, source_turn_id, agent_instance_id, "main"),
            );
        });
        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "root",
            2,
            std::time::Duration::ZERO,
        ))
    }
}

#[tokio::test]
async fn turn_submit_while_active_is_queued_until_prior_turn_terminals() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let runner = GatedTurnRunner::new();
    let prompts = Arc::clone(&runner.prompts);
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(runner.clone()));

    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("unexpected {other:?}"),
    };

    let first = {
        let server = server.clone();
        let session_id = session_id.clone();
        tokio::spawn(async move {
            server
                .handle_command(Command::TurnSubmit {
                    command_id: "submit-1".into(),
                    session_id,
                    text: "first".into(),
                })
                .await
        })
    };

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if !prompts.lock().unwrap().is_empty() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first turn should start");

    let second_events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit-2".into(),
            session_id: session_id.clone(),
            text: "second".into(),
        })
        .await;

    assert!(
        second_events.iter().any(|event| matches!(
            event,
            Event::Queue(piko_protocol::QueueEvent::Updated {
                next_turn_count: 1,
                ..
            })
        )),
        "second TurnSubmit must queue while prior turn is active; events={second_events:?}"
    );
    assert_eq!(
        prompts.lock().unwrap().as_slice(),
        ["first"],
        "second submit must not start a concurrent root turn"
    );

    runner.release();
    let first_events = first.await.expect("first turn join");
    assert!(
        first_events.iter().any(|event| matches!(
            event,
            Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
        )),
        "first turn should complete; events={first_events:?}"
    );

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if prompts.lock().unwrap().len() >= 2 {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("queued second turn should drain after first terminals");

    assert_eq!(
        prompts.lock().unwrap().as_slice(),
        ["first", "second"],
        "queued TurnSubmit must run after prior turn terminals"
    );
}
