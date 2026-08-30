#[path = "../support/mock_turn_runner.rs"]
mod mock_turn_runner;
#[path = "../support/mod.rs"]
mod support;

use std::sync::Arc;

use async_trait::async_trait;
use mock_turn_runner::MockAgentRunRunner;
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::HostServer;
use piko_orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::SessionRuntimeSnapshot;
use support::{MockSessionPublisher, execution_running, execution_succeeded, success_report};
use tokio_stream::StreamExt;

#[derive(Clone, Default)]
struct RecoveringAgentRunRunner {
    harness: crate::support::MockRunHarness,
    agent_instance_id: Arc<std::sync::Mutex<Option<String>>>,
    input_id: Arc<std::sync::Mutex<Option<String>>>,
    completion_tx: Arc<
        std::sync::Mutex<
            Option<tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>>,
        >,
    >,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
}

#[async_trait]
impl AgentRunRunner for RecoveringAgentRunRunner {
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
        let root_agent_instance_id = input
            .caller_agent_instance_id
            .clone()
            .unwrap_or_else(|| format!("agent_{}_root", input.session_id));
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        *self.agent_instance_id.lock().unwrap() = Some(root_agent_instance_id.clone());
        *self.input_id.lock().unwrap() = Some(input_id.clone());
        let (publisher, subscription) = MockSessionPublisher::new(session_id.clone());
        self.publishers.lock().unwrap().push(publisher.clone());
        let (completion_tx, completion_rx) = support::test_oneshot();
        *self.completion_tx.lock().unwrap() = Some(completion_tx);
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher,
        );
        let publish_agent_instance_id = root_agent_instance_id.clone();
        if let Some(publisher) = self.publishers.lock().unwrap().last().cloned() {
            tokio::spawn(async move {
                publisher.publish(publish_agent_instance_id, "main", 1, execution_running());
                publisher.require_snapshot(piko_orchd_api::SnapshotRequiredReason::CursorExpired);
            });
        }
        Ok(piko_protocol::AgentInputReceipt {
            input_id,
            request_id: input.request_id,
            session_id,
            agent_instance_id: root_agent_instance_id,
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
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn recover_observation(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
    ) -> Result<(SessionRuntimeSnapshot, SessionSubscription), piko_hostd::api::ProtocolError> {
        let agent_instance_id = self.agent_instance_id.lock().unwrap().clone().unwrap();
        let (publisher, subscription) = MockSessionPublisher::new(session_id.to_string());
        self.publishers.lock().unwrap().push(publisher.clone());
        let cursor = subscription.cursor.clone();
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: cursor.epoch.clone(),
            seq: 0,
        };
        let recovered_agent_instance_id = agent_instance_id.clone();
        let completion_tx = self.completion_tx.lock().unwrap().take();
        let completion_input_id = self.input_id.lock().unwrap().clone().unwrap();
        tokio::spawn(async move {
            publisher.publish(
                recovered_agent_instance_id.clone(),
                "main",
                2,
                execution_succeeded(),
            );
            if let Some(completion_tx) = completion_tx {
                let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
                    input_id: completion_input_id,
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
