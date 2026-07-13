#[path = "support/mock_session.rs"]
mod mock_session;

use std::sync::Arc;

use async_trait::async_trait;
use hostd::api::ServerMessage as Event;
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use hostd::protocol::HostServer;
use mock_session::MockSessionPublisher;

#[derive(Clone, Default)]
struct CancellableTurnRunner {
    active: Arc<std::sync::Mutex<Option<CancellableRun>>>,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

struct CancellableRun {
    session_id: String,
    turn_id: String,
    barrier: piko_protocol::agent_runtime::SessionCursor,
    completion_tx: tokio::sync::oneshot::Sender<hostd::ports::TurnRunCompletion>,
}

impl CancellableTurnRunner {
    fn finish_cancelled(&self) {
        let run = self.active.lock().unwrap().take().unwrap();
        let _ = run.completion_tx.send(hostd::ports::TurnRunCompletion {
            session_id: run.session_id,
            turn_id: run.turn_id,
            root_agent_instance_id: "root".into(),
            result: Ok(piko_protocol::AgentRunReport {
                agent_instance_id: "root".into(),
                report_id: "report-cancelled".into(),
                outcome: piko_protocol::ExecutionOutcome::Cancelled {
                    reason: Some("cancelled by test".into()),
                },
                summary: "cancelled".into(),
                usage: Default::default(),
                artifacts: Vec::new(),
            }),
            observation_barrier: run.barrier,
        });
    }
}

#[async_trait]
impl TurnRunner for CancellableTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        self.publishers.lock().unwrap().push(publisher.clone());
        publisher.publish(
            "root",
            "main",
            0,
            piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                resolution: serde_json::json!({"marker": "active"}),
            },
        );
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: subscription.cursor.epoch.clone(),
            seq: 1,
        };
        let (completion_tx, completion) = tokio::sync::oneshot::channel();
        *self.active.lock().unwrap() = Some(CancellableRun {
            session_id: input.session_id.clone(),
            turn_id: input.turn_id.clone(),
            barrier,
            completion_tx,
        });
        Ok(TurnRunHandle {
            session_id: input.session_id,
            turn_id: input.turn_id,
            observation: subscription,
            completion,
        })
    }

    async fn cancel_turn_run(&self, session_id: &str, turn_id: &str) -> bool {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|run| run.session_id == session_id && run.turn_id == turn_id)
    }
}

#[tokio::test]
async fn cancellation_acceptance_waits_for_durable_cancelled_report() {
    let runner = Arc::new(CancellableTurnRunner::default());
    let server = HostServer::with_turn_runner(runner.clone());
    let created = server
        .handle_command(hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let server_for_turn = server.clone();
    let turn_session_id = session_id.clone();
    let turn = tokio::spawn(async move {
        server_for_turn
            .handle_command(hostd::api::Command::TurnSubmit {
                command_id: "submit".into(),
                session_id: turn_session_id,
                text: "wait".into(),
            })
            .await
    });
    let turn_id = loop {
        let refresh = server
            .handle_command(hostd::api::Command::StateSnapshot {
                command_id: "snapshot".into(),
                session_id: session_id.clone(),
            })
            .await;
        let found = refresh.iter().find_map(|event| match event {
            Event::SessionReconciled(reconciled) => reconciled
                .snapshot
                .active_turn
                .as_ref()
                .map(|turn| turn.turn_id.clone()),
            _ => None,
        });
        if let Some(turn_id) = found {
            break turn_id;
        }
        tokio::task::yield_now().await;
    };

    let cancel = server
        .handle_command(hostd::api::Command::TurnCancel {
            command_id: "cancel".into(),
            session_id: session_id.clone(),
            turn_id,
        })
        .await;
    assert!(cancel.iter().all(|event| !matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Cancelled { .. })
    )));

    runner.finish_cancelled();
    let terminal = turn.await.unwrap();
    assert!(terminal.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Cancelled { .. })
    )));
}

struct ChildReportRunner;

#[async_trait]
impl TurnRunner for ChildReportRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let barrier = subscription.cursor.clone();
        let (completion_tx, completion) = tokio::sync::oneshot::channel();
        let session_id = input.session_id.clone();
        let turn_id = input.turn_id.clone();
        tokio::spawn(async move {
            let _publisher = publisher;
            let _ = completion_tx.send(hostd::ports::TurnRunCompletion {
                session_id,
                turn_id,
                root_agent_instance_id: "root".into(),
                result: Ok(success_report("child")),
                observation_barrier: barrier,
            });
        });
        Ok(TurnRunHandle {
            session_id: input.session_id,
            turn_id: input.turn_id,
            observation: subscription,
            completion,
        })
    }
}

#[tokio::test]
async fn child_agent_report_cannot_complete_root_turn() {
    let server = HostServer::with_turn_runner(Arc::new(ChildReportRunner));
    let created = server
        .handle_command(hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let events = server
        .handle_command(hostd::api::Command::TurnSubmit {
            command_id: "submit".into(),
            session_id,
            text: "run".into(),
        })
        .await;

    assert!(events.iter().all(|event| !matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::CommandResponse { result: Err(error), .. }
            if error.contains("root Agent report identity mismatch")
    )));
}

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .unwrap()
}

fn success_report(agent_instance_id: impl Into<String>) -> piko_protocol::AgentRunReport {
    piko_protocol::AgentRunReport {
        agent_instance_id: agent_instance_id.into(),
        report_id: "report-success".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    }
}
