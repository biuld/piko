mod support;

use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::api::ServerMessage as Event;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use support::{MockSessionPublisher, running_agent_info, test_agent_run_process};

#[derive(Clone, Default)]
struct SteerAgentRunRunner {
    active: Arc<std::sync::Mutex<Option<ActiveRun>>>,
    inputs: Arc<std::sync::Mutex<Vec<piko_protocol::MessageContent>>>,
    steers: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    raw_steers: Arc<std::sync::Mutex<Vec<piko_protocol::MessageContent>>>,
    accept_steer: bool,
}

struct ActiveRun {
    session_id: String,
    turn_id: String,
    agent_instance_id: String,
    _keep_open: tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>,
}

#[async_trait]
impl AgentRunRunner for SteerAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
        self.inputs.lock().unwrap().push(input.content.clone());
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        publisher.publish(
            "root",
            "main",
            0,
            piko_protocol::agent_runtime::SessionEvent::InteractionResolved {
                interaction_id: "active".into(),
                status: piko_protocol::UserInteractionStatus::Submitted,
            },
        );
        let (completion_tx, completion) = support::test_oneshot();
        *self.active.lock().unwrap() = Some(ActiveRun {
            session_id: input.session_id.clone(),
            turn_id: input.operation_id.clone(),
            agent_instance_id: input.agent_instance_id.clone(),
            _keep_open: completion_tx,
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
                input_id: input.operation_id.clone(),
                request_id: input.operation_id,
                session_id: input.session_id,
                agent_instance_id: input.agent_instance_id,
                disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
                queued_position: None,
            },
            process: test_agent_run_process(started, completion),
        })
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let running = self.active.lock().unwrap().as_ref().is_some_and(|run| {
            run.session_id == input.session_id && run.agent_instance_id == input.agent_instance_id
        });
        if !running || !self.accept_steer {
            return Err(piko_hostd::api::ProtocolError::InvalidCommand(
                "steer rejected".into(),
            ));
        }
        self.raw_steers.lock().unwrap().push(input.content.clone());
        self.steers.lock().unwrap().push((
            input.agent_instance_id.clone(),
            match &input.content {
                piko_protocol::MessageContent::String(text) => text.clone(),
                piko_protocol::MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .map(piko_protocol::ContentBlock::text_projection)
                    .collect::<Vec<_>>()
                    .join("\n"),
            },
        ));
        Ok(piko_protocol::AgentInputReceipt {
            input_id: input.input_id,
            request_id: input.request_id,
            session_id: input.session_id,
            agent_instance_id: input.agent_instance_id,
            disposition: piko_protocol::AgentInputDisposition::PendingSteer,
            queued_position: None,
        })
    }

    async fn cancel_agent_run(&self, operation: &piko_hostd::ports::AgentOperationAddress) -> bool {
        self.active.lock().unwrap().as_ref().is_some_and(|run| {
            run.session_id == operation.session_id && run.turn_id == operation.operation_id
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
async fn structured_image_content_reaches_start_and_steer_runner_ports() {
    let runner = Arc::new(SteerAgentRunRunner {
        accept_steer: true,
        ..SteerAgentRunRunner::default()
    });
    let server = HostServer::with_turn_runner(runner.clone());
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create-image".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let agent = format!("agent_{session_id}_root");
    let image_content =
        piko_protocol::MessageContent::Blocks(vec![piko_protocol::ContentBlock::Image {
            data: "AA==".into(),
            mime_type: "image/png".into(),
        }]);
    let turn_server = server.clone();
    let turn_session = session_id.clone();
    let turn_agent = agent.clone();
    let turn_content = image_content.clone();
    let turn = tokio::spawn(async move {
        turn_server
            .handle_command(piko_hostd::api::Command::submit_follow_up(
                "submit-image",
                turn_session,
                turn_agent,
                turn_content,
            ))
            .await
    });
    while runner.inputs.lock().unwrap().is_empty() {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        runner.inputs.lock().unwrap().as_slice(),
        std::slice::from_ref(&image_content)
    );

    let steered = server
        .handle_command(piko_hostd::api::Command::submit_steer(
            "steer-image",
            session_id,
            agent,
            image_content.clone(),
        ))
        .await;
    assert!(steered.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { .. }),
            ..
        }
    )));
    assert_eq!(
        runner.raw_steers.lock().unwrap().as_slice(),
        &[image_content]
    );
    turn.abort();
}

async fn start_running_turn(
    server: &HostServer,
    session_id: &str,
    agent_instance_id: &str,
) -> (tokio::task::JoinHandle<Vec<Event>>, String) {
    let server_for_turn = server.clone();
    let turn_session_id = session_id.to_string();
    let target = agent_instance_id.to_string();
    let turn = tokio::spawn(async move {
        server_for_turn
            .handle_command(piko_hostd::api::Command::submit_follow_up(
                "submit",
                turn_session_id,
                target,
                piko_protocol::MessageContent::String("wait".into()),
            ))
            .await
    });
    let turn_id = loop {
        let refresh = server
            .handle_command(piko_hostd::api::Command::StateSnapshot {
                command_id: "snapshot".into(),
                session_id: session_id.to_string(),
            })
            .await;
        let found = refresh.iter().find_map(|event| match event {
            Event::SessionReconciled(reconciled) => {
                running_root_input_id(reconciled, agent_instance_id)
            }
            _ => None,
        });
        if let Some(turn_id) = found {
            break turn_id;
        }
        tokio::task::yield_now().await;
    };
    (turn, turn_id)
}

fn running_root_input_id(
    reconciled: &piko_protocol::SessionReconciledEvent,
    agent_instance_id: &str,
) -> Option<String> {
    if let Some(root_input_id) = reconciled
        .snapshot
        .agent_work
        .iter()
        .find(|work| work.agent_instance_id == agent_instance_id)
        .and_then(|work| {
            work.active_work
                .as_ref()
                .map(|active| active.root_input_id.clone())
        })
    {
        return Some(root_input_id);
    }
    reconciled
        .agents
        .iter()
        .any(|agent| {
            agent.agent_instance_id == agent_instance_id
                && matches!(
                    agent.activity,
                    piko_protocol::AgentActivity::Running
                        | piko_protocol::AgentActivity::WaitingForApproval
                        | piko_protocol::AgentActivity::Cancelling
                )
        })
        .then(String::new)
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

#[tokio::test]
async fn queue_steer_fails_closed_when_idle() {
    let runner = Arc::new(SteerAgentRunRunner {
        accept_steer: true,
        ..SteerAgentRunRunner::default()
    });
    let server = HostServer::with_turn_runner(runner);
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let agent = format!("agent_{session_id}_root");
    let events = server
        .handle_command(piko_hostd::api::Command::submit_steer(
            "steer",
            session_id,
            agent,
            piko_protocol::MessageContent::String("go left".into()),
        ))
        .await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::CommandResponse { result: Err(error), .. }
            if error.contains("not running")
    )));
    assert!(events.iter().all(|event| !matches!(event, Event::Queue(_))));
}

#[tokio::test]
async fn queue_steer_fails_when_runtime_rejects() {
    let runner = Arc::new(SteerAgentRunRunner {
        accept_steer: false,
        ..SteerAgentRunRunner::default()
    });
    let server = HostServer::with_turn_runner(runner);
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let agent = format!("agent_{session_id}_root");
    let (turn, _turn_id) = start_running_turn(&server, &session_id, &agent).await;
    let events = server
        .handle_command(piko_hostd::api::Command::submit_steer(
            "steer",
            session_id.clone(),
            agent,
            piko_protocol::MessageContent::String("go left".into()),
        ))
        .await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::CommandResponse { result: Err(error), .. }
            if error.contains("steer rejected")
    )));
    assert!(events.iter().all(|event| !matches!(event, Event::Queue(_))));
    drop(turn);
}

#[tokio::test]
async fn queue_steer_injects_only_after_runtime_accepts() {
    let runner = Arc::new(SteerAgentRunRunner {
        accept_steer: true,
        ..SteerAgentRunRunner::default()
    });
    let server = HostServer::with_turn_runner(runner.clone());
    let created = server
        .handle_command(piko_hostd::api::Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let agent = format!("agent_{session_id}_root");
    let (_turn, _turn_id) = start_running_turn(&server, &session_id, &agent).await;

    let steered = server
        .handle_command(piko_hostd::api::Command::submit_steer(
            "steer",
            session_id,
            agent.clone(),
            piko_protocol::MessageContent::String("go left".into()),
        ))
        .await;
    assert!(steered.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInputSubmitted { .. }),
            ..
        }
    )));
    assert!(
        steered
            .iter()
            .any(|event| matches!(event, Event::SessionReconciled(_)))
    );
    assert_eq!(
        runner.steers.lock().unwrap().as_slice(),
        &[(agent, "go left".into())]
    );
}
