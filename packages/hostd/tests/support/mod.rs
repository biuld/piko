#![allow(clippy::disallowed_methods)]
#![allow(dead_code)]

pub mod mock_session;

pub use mock_session::MockSessionPublisher;

pub fn test_oneshot<T>() -> (
    tokio::sync::oneshot::Sender<T>,
    tokio::sync::oneshot::Receiver<T>,
) {
    tokio::sync::oneshot::channel()
}

pub struct TestAgentRunProcess {
    started: tokio::sync::oneshot::Receiver<piko_orchd_api::SessionSubscription>,
    completion: tokio::sync::oneshot::Receiver<piko_hostd::ports::AgentRunCompletion>,
}

#[async_trait::async_trait]
impl piko_hostd::ports::AgentRunProcess for TestAgentRunProcess {
    async fn wait_started(
        &mut self,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        (&mut self.started).await.map_err(|_| {
            piko_hostd::api::ProtocolError::ObservationFailed("test start signal closed".into())
        })
    }

    async fn wait_completion(
        self: Box<Self>,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        self.completion.await.map_err(|_| {
            piko_hostd::api::ProtocolError::ObservationFailed(
                "test completion signal closed".into(),
            )
        })
    }
}

pub fn test_agent_run_process(
    started: tokio::sync::oneshot::Receiver<piko_orchd_api::SessionSubscription>,
    completion: tokio::sync::oneshot::Receiver<piko_hostd::ports::AgentRunCompletion>,
) -> Box<dyn piko_hostd::ports::AgentRunProcess> {
    Box::new(TestAgentRunProcess {
        started,
        completion,
    })
}

pub fn successful_turn_run(
    subscription: piko_orchd_api::SessionSubscription,
    session_id: impl Into<String>,
    turn_id: impl Into<String>,
    root_agent_instance_id: impl Into<String>,
    barrier_seq: u64,
    delay: std::time::Duration,
) -> piko_hostd::ports::AgentRunHandle {
    let session_id = session_id.into();
    let turn_id = turn_id.into();
    let root_agent_instance_id = root_agent_instance_id.into();
    let barrier = piko_protocol::agent_runtime::SessionCursor {
        epoch: subscription.cursor.epoch.clone(),
        seq: barrier_seq,
    };
    let (completion_tx, completion) = test_oneshot();
    let (started_tx, started) = test_oneshot();
    let _ = started_tx.send(subscription);
    let handle_root_agent_instance_id = root_agent_instance_id.clone();
    let address = piko_hostd::ports::AgentOperationAddress {
        session_id: session_id.clone(),
        operation_id: turn_id.clone(),
        agent_instance_id: handle_root_agent_instance_id,
    };
    let completion_address = address.clone();
    let report_agent_instance_id = root_agent_instance_id.clone();
    tokio::spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
            address: completion_address,
            result: Ok(success_report(report_agent_instance_id)),
            observation_barrier: barrier,
        });
    });
    piko_hostd::ports::AgentRunHandle {
        address,
        receipt: piko_protocol::AgentInputReceipt {
            request_id: turn_id,
            session_id,
            agent_instance_id: root_agent_instance_id.clone(),
            disposition: piko_protocol::InputDisposition::Accepted,
        },
        process: test_agent_run_process(started, completion),
    }
}

pub fn success_report(agent_instance_id: impl Into<String>) -> piko_protocol::AgentRunReport {
    piko_protocol::AgentRunReport {
        agent_instance_id: agent_instance_id.into(),
        report_id: format!("report_{}", uuid::Uuid::new_v4()),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: Default::default(),
        },
        summary: "done".into(),
        usage: Default::default(),
        artifacts: Vec::new(),
    }
}

use piko_protocol::agent_runtime::SessionEvent;
pub fn execution_running() -> SessionEvent {
    SessionEvent::InteractionResolved {
        interaction_id: "running".into(),
        status: piko_protocol::UserInteractionStatus::Submitted,
    }
}

pub fn execution_succeeded() -> SessionEvent {
    SessionEvent::InteractionResolved {
        interaction_id: "completed".into(),
        status: piko_protocol::UserInteractionStatus::Submitted,
    }
}
