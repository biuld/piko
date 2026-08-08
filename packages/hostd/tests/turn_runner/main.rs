#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::ports::{AgentRunHandle, AgentRunInput, AgentRunRunner};
use piko_hostd::protocol::HostServer;
use piko_orchd_api::SessionSubscription;
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
        std::sync::Mutex<
            Option<tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>>,
        >,
    >,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

#[async_trait]
impl AgentRunRunner for RecoveringAgentRunRunner {
    async fn run_agent(
        &self,
        input: AgentRunInput,
    ) -> Result<AgentRunHandle, piko_hostd::api::ProtocolError> {
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
            publisher.require_snapshot(piko_orchd_api::SnapshotRequiredReason::CursorExpired);
        });
        let (started_tx, started) = support::test_oneshot();
        let _ = started_tx.send(subscription);
        Ok(AgentRunHandle {
            address: piko_hostd::ports::AgentOperationAddress {
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
        operation: &piko_hostd::ports::AgentOperationAddress,
    ) -> Result<(SessionRuntimeSnapshot, SessionSubscription), piko_hostd::api::ProtocolError> {
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
                let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                    address: piko_hostd::ports::AgentOperationAddress {
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
        Vec<piko_hostd::api::ApprovalSnapshot>,
        Vec<piko_hostd::api::UserInteractionSnapshot>,
    ) {
        (
            vec![piko_hostd::api::ApprovalSnapshot {
                approval_id: "approval-recovered".into(),
                agent_instance_id: self
                    .agent_instance_id
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| format!("agent_{session_id}_root")),
                tool_name: "bash".into(),
                request: serde_json::json!({"cmd": "pwd"}),
                prompt: None,
                status: piko_hostd::api::ApprovalStatus::Pending,
            }],
            Vec::new(),
        )
    }
}

mod gated;
mod gated_tests;
mod recovery_tests;

pub(crate) use gated::GatedAgentRunRunner;
