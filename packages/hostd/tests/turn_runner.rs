#[path = "support/mock_turn_runner.rs"]
mod mock_turn_runner;
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use hostd::protocol::HostServer;
use mock_turn_runner::MockAgentRunRunner;
use orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::SessionRuntimeSnapshot;
use support::{
    MockSessionPublisher, execution_running, execution_succeeded, success_report,
    successful_turn_run, test_agent_run_process,
};
use tokio_stream::StreamExt;

#[derive(Clone, Default)]
struct RecoveringAgentRunRunner {
    agent_instance_id: Arc<std::sync::Mutex<Option<String>>>,
    turn_id: Arc<std::sync::Mutex<Option<String>>>,
    completion_tx: Arc<
        std::sync::Mutex<Option<tokio::sync::oneshot::Sender<hostd::ports::AgentRunCompletion>>>,
    >,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

#[async_trait]
impl AgentRunRunner for RecoveringAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, hostd::api::ProtocolError> {
        let root_agent_instance_id = input
            .resume_agent
            .as_ref()
            .map(|agent| agent.agent_instance_id.clone())
            .unwrap_or_else(|| format!("agent_{}_root", input.session_id));
        *self.agent_instance_id.lock().unwrap() = Some(root_agent_instance_id.clone());
        *self.turn_id.lock().unwrap() = Some(input.operation_id.clone());
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        self.publishers.lock().unwrap().push(publisher.clone());
        let (completion_tx, completion) = support::test_oneshot();
        *self.completion_tx.lock().unwrap() = Some(completion_tx);
        let publish_agent_instance_id = root_agent_instance_id.clone();
        tokio::spawn(async move {
            publisher.publish(
                publish_agent_instance_id.clone(),
                "main",
                1,
                execution_running(),
            );
            publisher.require_snapshot(orchd_api::SnapshotRequiredReason::CursorExpired);
        });
        let (started_tx, started) = support::test_oneshot();
        let _ = started_tx.send(subscription);
        Ok(AgentRunHandle {
            address: hostd::ports::AgentOperationAddress {
                session_id: input.session_id.clone(),
                operation_id: input.operation_id.clone(),
                agent_instance_id: root_agent_instance_id.clone(),
            },
            receipt: piko_protocol::AgentInputReceipt {
                request_id: input.operation_id,
                session_id: input.session_id,
                agent_instance_id: root_agent_instance_id,
                disposition: piko_protocol::InputDisposition::Accepted,
            },
            process: test_agent_run_process(started, completion),
        })
    }

    async fn recover_observation(
        &self,
        operation: &hostd::ports::AgentOperationAddress,
    ) -> Result<(SessionRuntimeSnapshot, SessionSubscription), hostd::api::ProtocolError> {
        let session_id = &operation.session_id;
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
                execution_succeeded(),
            );
            if let Some(completion_tx) = completion_tx {
                let _ = completion_tx.send(hostd::ports::AgentRunCompletion {
                    address: hostd::ports::AgentOperationAddress {
                        session_id: recovered_session_id,
                        operation_id: completion_turn_id,
                        agent_instance_id: recovered_agent_instance_id.clone(),
                    },
                    result: Ok(success_report(&recovered_agent_instance_id)),
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
        session_id: &str,
    ) -> (
        Vec<hostd::api::ApprovalSnapshot>,
        Vec<hostd::api::UserInteractionSnapshot>,
    ) {
        (
            vec![hostd::api::ApprovalSnapshot {
                approval_id: "approval-recovered".into(),
                agent_instance_id: self
                    .agent_instance_id
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| format!("agent_{session_id}_root")),
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
    let runner = MockAgentRunRunner;
    let subscription = runner
        .run_agent(AgentRunInput {
            session_id: "session-test".into(),
            operation_id: "turn-test".into(),
            agent_instance_id: "agent_session-test_root".into(),
            prompt: "hello".into(),
            source_turn_id: Some("turn-test".into()),
            prompt_resources: Some(piko_protocol::PromptResourceSnapshot {
                product_instructions: "system prompt".into(),
                ..Default::default()
            }),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            resume_agent: None,
        })
        .await
        .unwrap();

    let mut process = subscription.process;
    let mut output = process.wait_started().await.unwrap().output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn mock_turn_with_storage_populates_state() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockAgentRunRunner));

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
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
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
    let runner = MockAgentRunRunner;

    let subscription = runner
        .run_agent(AgentRunInput {
            session_id: "session-test".into(),
            operation_id: "turn-test".into(),
            agent_instance_id: "agent_session-test_root".into(),
            prompt: "hello".into(),
            source_turn_id: Some("turn-test".into()),
            prompt_resources: Some(piko_protocol::PromptResourceSnapshot {
                product_instructions: "system prompt".into(),
                ..Default::default()
            }),
            cwd: "".into(),
            active_tool_names: None,
            session_dir: std::env::temp_dir().join("piko-test-turn-runner"),
            resume_agent: None,
        })
        .await
        .unwrap();

    let mut process = subscription.process;
    let mut output = process.wait_started().await.unwrap().output;
    assert!(output.next().await.is_some());
}

#[tokio::test]
async fn snapshot_required_reconciles_and_resubscribes_without_losing_turn() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let temp = tempfile::tempdir().unwrap();
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(RecoveringAgentRunRunner::default()),
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
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    assert!(
        events.iter().any(|event| matches!(event,
            Event::SessionReconciled(reconciled)
                if reconciled.reason == piko_protocol::ReconcileReason::RetentionExhausted
                    && reconciled.snapshot.pending_approvals.len() == 1
                    && reconciled.snapshot.active_turns.iter().any(|turn|
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
struct GatedAgentRunRunner {
    released: Arc<(std::sync::Mutex<bool>, tokio::sync::Notify)>,
    prompts: Arc<std::sync::Mutex<Vec<String>>>,
    submissions: Arc<std::sync::atomic::AtomicUsize>,
}

impl GatedAgentRunRunner {
    fn new() -> Self {
        Self {
            released: Arc::new((std::sync::Mutex::new(false), tokio::sync::Notify::new())),
            prompts: Arc::new(std::sync::Mutex::new(Vec::new())),
            submissions: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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
impl AgentRunRunner for GatedAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let disposition = if self
            .submissions
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 0
        {
            piko_protocol::InputDisposition::Accepted
        } else {
            piko_protocol::InputDisposition::Queued
        };
        if disposition == piko_protocol::InputDisposition::Accepted {
            self.prompts.lock().unwrap().push(input.prompt.clone());
        }
        let (started_tx, started) = support::test_oneshot();
        let epoch = subscription.cursor.epoch.clone();
        let queued_start = if disposition == piko_protocol::InputDisposition::Accepted {
            let _ = started_tx.send(subscription);
            None
        } else {
            Some((started_tx, subscription))
        };
        let (completion_tx, completion) = support::test_oneshot();
        let runner = self.clone();
        let session_id = input.session_id.clone();
        let operation_id = input.operation_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let prompt = input.prompt.clone();
        let address = hostd::ports::AgentOperationAddress {
            session_id: session_id.clone(),
            operation_id: operation_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
        };
        let completion_address = address.clone();
        tokio::spawn(async move {
            runner.wait_until_released().await;
            if let Some((started_tx, subscription)) = queued_start {
                runner.prompts.lock().unwrap().push(prompt);
                let _ = started_tx.send(subscription);
            }
            publisher.publish(agent_instance_id.clone(), "main", 1, execution_running());
            publisher.publish(agent_instance_id.clone(), "main", 2, execution_succeeded());
            let _ = completion_tx.send(hostd::ports::AgentRunCompletion {
                address: completion_address,
                result: Ok(success_report(agent_instance_id)),
                observation_barrier: piko_protocol::agent_runtime::SessionCursor { epoch, seq: 2 },
            });
        });
        Ok(AgentRunHandle {
            address,
            receipt: piko_protocol::AgentInputReceipt {
                request_id: operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition,
            },
            process: test_agent_run_process(started, completion),
        })
    }
}

#[tokio::test]
async fn root_chat_while_active_is_queued_until_prior_turn_terminals() {
    use hostd::api::{Command, ServerMessage as Event};
    use hostd::infra::storage::JsonlSessionRepository;

    let runner = GatedAgentRunRunner::new();
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
                .handle_command(Command::ChatSubmit {
                    command_id: "submit-1".into(),
                    target_agent_instance_id: format!("agent_{session_id}_root"),
                    session_id: session_id.clone(),
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

    let mut second = server.handle_command_stream(Command::ChatSubmit {
        command_id: "submit-2".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        text: "second".into(),
    });
    let mut second_events = Vec::new();
    for _ in 0..2 {
        second_events.push(
            tokio::time::timeout(std::time::Duration::from_secs(2), second.recv())
                .await
                .expect("queued receipt events should arrive")
                .expect("queued command stream should remain open"),
        );
    }

    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            command_id,
            result: Ok(hostd::api::CommandResult::Empty),
        } if command_id == "submit-2"
    )));
    assert!(
        second_events.iter().any(|event| matches!(
            event,
            Event::TurnLifecycle(piko_protocol::TurnEvent::Queued {
                agent_instance_id,
                ..
            }) if agent_instance_id == &format!("agent_{session_id}_root")
        )),
        "second root chat must queue while prior turn is active; events={second_events:?}"
    );
    assert_eq!(
        prompts.lock().unwrap().as_slice(),
        ["first"],
        "second submit must not start a concurrent root turn"
    );

    runner.release();
    while let Some(event) = second.recv().await {
        second_events.push(event);
    }
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
        "queued root chat must run after prior turn terminals"
    );
    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Started { .. })
    )));
    assert!(second_events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}
