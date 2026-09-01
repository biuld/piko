mod support;

use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::api::ServerMessage as Event;
use piko_hostd::ports::{AgentRunCompletion, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use support::{MockSessionPublisher, running_agent_info};

#[derive(Clone, Default)]
struct CancellableAgentRunRunner {
    harness: crate::support::MockRunHarness,
    active: Arc<std::sync::Mutex<Option<CancellableRun>>>,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

struct CancellableRun {
    session_id: String,
    input_id: String,
    agent_instance_id: String,
    epoch: String,
    completion_tx: tokio::sync::oneshot::Sender<AgentRunCompletion>,
}

impl CancellableAgentRunRunner {
    fn finish_cancelled(&self) {
        let run = self.active.lock().unwrap().take().unwrap();
        let agent_instance_id = run.agent_instance_id;
        let input_id = run.input_id;
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: run.epoch,
            seq: 1,
        };
        let _ = run.completion_tx.send(AgentRunCompletion {
            input_id: input_id.clone(),
            result: Ok(piko_protocol::AgentWorkReport {
                agent_instance_id: agent_instance_id.clone(),
                root_input_id: input_id.clone(),
                report_id: "report-cancelled".into(),
                outcome: piko_protocol::AgentWorkOutcome::Cancelled {
                    reason: Some("cancelled by test".into()),
                },
                summary: "cancelled".into(),
                usage: Default::default(),
                artifacts: Vec::new(),
            }),
            observation_barrier: barrier,
        });
    }
}

#[async_trait]
impl AgentRunRunner for CancellableAgentRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        _session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        self.publishers.lock().unwrap().push(Arc::clone(&publisher));
        publisher.publish(
            "root",
            "main",
            0,
            piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                interaction_id: "active".into(),
                status: piko_protocol::UserInteractionStatus::Submitted,
            },
        );
        let (completion_tx, completion_rx) = support::test_oneshot();
        let epoch = publisher.cursor().epoch;
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher,
        );
        *self.active.lock().unwrap() = Some(CancellableRun {
            session_id,
            input_id: input_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            epoch,
            completion_tx,
        });
        Ok(piko_protocol::AgentInputReceipt {
            input_id,
            request_id: input.request_id,
            session_id: input.session_id,
            agent_instance_id,
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        })
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }

    async fn interrupt_agent(&self, session_id: &str, agent_instance_id: &str) -> bool {
        self.active.lock().unwrap().as_ref().is_some_and(|run| {
            run.session_id == session_id && run.agent_instance_id == agent_instance_id
        })
    }

    async fn has_active_session_run(&self, session_id: &str) -> bool {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|run| run.session_id == session_id)
    }

    async fn list_agent_instances(
        &self,
        session_id: &str,
    ) -> Option<Vec<piko_protocol::AgentInfo>> {
        self.active.lock().unwrap().as_ref().and_then(|run| {
            (run.session_id == session_id).then(|| {
                vec![running_agent_info(
                    session_id,
                    run.agent_instance_id.clone(),
                )]
            })
        })
    }
}

#[tokio::test]
async fn agent_interrupt_preserves_turn_terminal_authority() {
    let runner = Arc::new(CancellableAgentRunRunner::default());
    let server = HostServer::with_agent_runner(runner.clone());
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
            .handle_command(piko_hostd::api::Command::submit_follow_up(
                "submit",
                turn_session_id.clone(),
                format!("agent_{turn_session_id}_root"),
                piko_protocol::MessageContent::String("wait".into()),
            ))
            .await
    });
    let _turn_id = loop {
        let refresh = server
            .handle_command(piko_hostd::api::Command::StateSnapshot {
                command_id: "snapshot".into(),
                session_id: session_id.clone(),
            })
            .await;
        let found = refresh.iter().find_map(|event| match event {
            Event::SessionReconciled(reconciled) => {
                let agent = format!("agent_{session_id}_root");
                reconciled
                    .snapshot
                    .agent_work
                    .iter()
                    .find(|work| work.agent_instance_id == agent)
                    .and_then(|work| {
                        work.active_work
                            .as_ref()
                            .map(|active| active.root_input_id.clone())
                    })
                    .or_else(|| {
                        reconciled
                            .agents
                            .iter()
                            .any(|info| {
                                info.agent_instance_id == agent
                                    && matches!(
                                        info.activity,
                                        piko_protocol::AgentActivity::Running
                                            | piko_protocol::AgentActivity::WaitingForApproval
                                            | piko_protocol::AgentActivity::Cancelling
                                    )
                            })
                            .then(String::new)
                    })
            }
            _ => None,
        });
        if let Some(turn_id) = found {
            break turn_id;
        }
        tokio::task::yield_now().await;
    };

    let cancel = server
        .handle_command(piko_hostd::api::Command::AgentInterrupt {
            command_id: "interrupt".into(),
            session_id: session_id.clone(),
            agent_instance_id: format!("agent_{session_id}_root"),
        })
        .await;
    assert!(cancel.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInterrupted { accepted: true, .. }),
            ..
        }
    )));
    runner.finish_cancelled();
    let terminal = turn.await.unwrap();
    assert!(terminal.iter().any(|event| matches!(
        event,
        Event::SessionReconciled(reconciled)
            if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none())
    )));
}

#[derive(Default)]
struct RecordingInterruptRunner {
    targets: std::sync::Mutex<Vec<(String, String)>>,
    accepted: bool,
}

#[async_trait]
impl AgentRunRunner for RecordingInterruptRunner {
    async fn interrupt_agent(&self, session_id: &str, agent_instance_id: &str) -> bool {
        self.targets
            .lock()
            .unwrap()
            .push((session_id.to_string(), agent_instance_id.to_string()));
        self.accepted
    }
}

#[tokio::test]
async fn detached_agent_interrupt_is_forwarded_without_a_turn() {
    let runner = Arc::new(RecordingInterruptRunner {
        accepted: true,
        ..Default::default()
    });
    let server = HostServer::with_agent_runner(runner.clone());
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let response = server
        .handle_command(piko_hostd::api::Command::AgentInterrupt {
            command_id: "interrupt-child".into(),
            session_id: session_id.clone(),
            agent_instance_id: "agent-child".into(),
        })
        .await;

    assert_eq!(
        runner.targets.lock().unwrap().as_slice(),
        &[(session_id.clone(), "agent-child".into())]
    );
    assert!(response.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInterrupted {
                agent_instance_id,
                accepted: true,
                ..
            }),
            ..
        } if agent_instance_id == "agent-child"
    )));
}

#[tokio::test]
async fn idle_agent_interrupt_is_a_benign_unaccepted_result() {
    let runner = Arc::new(RecordingInterruptRunner::default());
    let server = HostServer::with_agent_runner(runner);
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let response = server
        .handle_command(piko_hostd::api::Command::AgentInterrupt {
            command_id: "interrupt-idle".into(),
            session_id,
            agent_instance_id: "agent-idle".into(),
        })
        .await;

    assert!(response.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInterrupted {
                accepted: false,
                ..
            }),
            ..
        }
    )));
    assert!(
        !response
            .iter()
            .any(|event| matches!(event, Event::SessionReconciled(_))),
        "idle interrupt must not push SessionReconciled"
    );
}

#[derive(Clone, Default)]
struct ChildReportRunner {
    harness: crate::support::MockRunHarness,
}

#[async_trait]
impl AgentRunRunner for ChildReportRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        _session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let (publisher, subscription) = MockSessionPublisher::new(session_id.clone());
        let (completion_tx, completion_rx) = support::test_oneshot();
        let barrier = publisher.cursor();
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher,
        );
        let completion_input_id = input_id.clone();
        tokio::spawn(async move {
            let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                input_id: completion_input_id,
                result: Ok(success_report("child")),
                observation_barrier: barrier,
            });
        });
        Ok(piko_protocol::AgentInputReceipt {
            input_id,
            request_id: input.request_id,
            session_id: input.session_id,
            agent_instance_id,
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        })
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }
}

#[tokio::test]
async fn mismatched_agent_report_cannot_complete_turn() {
    let server = HostServer::with_agent_runner(Arc::new(ChildReportRunner::default()));
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);

    let events = server
        .handle_command(piko_hostd::api::Command::submit_follow_up(
            "submit",
            session_id.clone(),
            format!("agent_{session_id}_root"),
            piko_protocol::MessageContent::String("run".into()),
        ))
        .await;

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

fn success_report(agent_instance_id: impl Into<String>) -> piko_protocol::AgentWorkReport {
    let agent_instance_id = agent_instance_id.into();
    piko_protocol::AgentWorkReport {
        agent_instance_id: agent_instance_id.clone(),
        root_input_id: agent_instance_id.clone(),
        report_id: "report-success".into(),
        outcome: piko_protocol::AgentWorkOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    }
}
