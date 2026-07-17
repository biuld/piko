mod support;

use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::api::ServerMessage as Event;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use support::{MockSessionPublisher, test_agent_run_process};

#[derive(Clone, Default)]
struct CancellableAgentRunRunner {
    active: Arc<std::sync::Mutex<Option<CancellableRun>>>,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

struct CancellableRun {
    session_id: String,
    turn_id: String,
    agent_instance_id: String,
    barrier: piko_protocol::agent_runtime::SessionCursor,
    completion_tx: tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>,
}

impl CancellableAgentRunRunner {
    fn finish_cancelled(&self) {
        let run = self.active.lock().unwrap().take().unwrap();
        let agent_instance_id = run.agent_instance_id;
        let _ = run
            .completion_tx
            .send(piko_hostd::ports::AgentRunCompletion {
                address: piko_hostd::ports::AgentOperationAddress {
                    session_id: run.session_id,
                    operation_id: run.turn_id,
                    agent_instance_id: agent_instance_id.clone(),
                },
                result: Ok(piko_protocol::AgentRunReport {
                    agent_instance_id: agent_instance_id.clone(),
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
impl AgentRunRunner for CancellableAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        self.publishers.lock().unwrap().push(publisher.clone());
        publisher.publish(
            "root",
            "main",
            0,
            piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                interaction_id: "active".into(),
                status: piko_protocol::UserInteractionStatus::Submitted,
            },
        );
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: subscription.cursor.epoch.clone(),
            seq: 1,
        };
        let (completion_tx, completion) = support::test_oneshot();
        *self.active.lock().unwrap() = Some(CancellableRun {
            session_id: input.session_id.clone(),
            turn_id: input.operation_id.clone(),
            agent_instance_id: input.agent_instance_id.clone(),
            barrier,
            completion_tx,
        });
        let (started_tx, started) = support::test_oneshot();
        let _ = started_tx.send(subscription);
        Ok(AgentRunHandle {
            address: piko_hostd::ports::AgentOperationAddress {
                session_id: input.session_id.clone(),
                operation_id: input.operation_id.clone(),
                agent_instance_id: input.agent_instance_id.clone(),
            },
            receipt: piko_protocol::AgentInputReceipt {
                request_id: input.operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition: piko_protocol::InputDisposition::Accepted,
            },
            process: test_agent_run_process(started, completion),
        })
    }

    async fn cancel_agent_run(&self, operation: &piko_hostd::ports::AgentOperationAddress) -> bool {
        self.active.lock().unwrap().as_ref().is_some_and(|run| {
            run.session_id == operation.session_id && run.turn_id == operation.operation_id
        })
    }
}

#[tokio::test]
async fn cancellation_acceptance_waits_for_durable_cancelled_report() {
    let runner = Arc::new(CancellableAgentRunRunner::default());
    let server = HostServer::with_turn_runner(runner.clone());
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let server_for_turn = server.clone();
    let turn_session_id = session_id.clone();
    let turn = tokio::spawn(async move {
        server_for_turn
            .handle_command(piko_hostd::api::Command::ChatSubmit {
                command_id: "submit".into(),
                target_agent_instance_id: format!("agent_{turn_session_id}_root"),
                session_id: turn_session_id.clone(),
                text: "wait".into(),
            })
            .await
    });
    let turn_id = loop {
        let refresh = server
            .handle_command(piko_hostd::api::Command::StateSnapshot {
                command_id: "snapshot".into(),
                session_id: session_id.clone(),
            })
            .await;
        let found = refresh.iter().find_map(|event| match event {
            Event::SessionReconciled(reconciled) => reconciled
                .snapshot
                .active_turns
                .first()
                .map(|turn| turn.turn_id.clone()),
            _ => None,
        });
        if let Some(turn_id) = found {
            break turn_id;
        }
        tokio::task::yield_now().await;
    };

    let cancel = server
        .handle_command(piko_hostd::api::Command::TurnCancel {
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
impl AgentRunRunner for ChildReportRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let barrier = subscription.cursor.clone();
        let (completion_tx, completion) = support::test_oneshot();
        let session_id = input.session_id.clone();
        let turn_id = input.operation_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        tokio::spawn(async move {
            let _publisher = publisher;
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                address: piko_hostd::ports::AgentOperationAddress {
                    session_id,
                    operation_id: turn_id,
                    agent_instance_id,
                },
                result: Ok(success_report("child")),
                observation_barrier: barrier,
            });
        });
        let (started_tx, started) = support::test_oneshot();
        let _ = started_tx.send(subscription);
        Ok(AgentRunHandle {
            address: piko_hostd::ports::AgentOperationAddress {
                session_id: input.session_id.clone(),
                operation_id: input.operation_id.clone(),
                agent_instance_id: input.agent_instance_id.clone(),
            },
            receipt: piko_protocol::AgentInputReceipt {
                request_id: input.operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition: piko_protocol::InputDisposition::Accepted,
            },
            process: test_agent_run_process(started, completion),
        })
    }
}

#[tokio::test]
async fn mismatched_agent_report_cannot_complete_turn() {
    let server = HostServer::with_turn_runner(Arc::new(ChildReportRunner));
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let events = server
        .handle_command(piko_hostd::api::Command::ChatSubmit {
            command_id: "submit".into(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            session_id: session_id.clone(),
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
            if error.contains("Agent report identity mismatch")
    )));
}

fn session_id_from(events: &[Event]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionCreated { session_id, .. }),
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
