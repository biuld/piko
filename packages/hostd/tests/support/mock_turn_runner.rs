use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::infra::storage::SessionStore;
use piko_hostd::ports::{AgentRunCompletion, AgentRunRunner, ResumeAgent};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{Message, MessageContent};

use crate::support::{MockRunHarness, MockSessionPublisher, success_report, test_oneshot};

/// A mock runner that admits user inputs, commits the user message durably, and
/// publishes a two-event observation (running → completed) with a successful
/// terminal report.
#[derive(Clone, Default)]
pub struct MockAgentRunRunner {
    harness: MockRunHarness,
    publishers: Arc<std::sync::Mutex<Vec<Arc<MockSessionPublisher>>>>,
    session_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

#[async_trait]
impl AgentRunRunner for MockAgentRunRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn recover_observation(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
    ) -> Result<
        (
            piko_protocol::agent_runtime::SessionRuntimeSnapshot,
            piko_orchd_api::SessionSubscription,
        ),
        piko_hostd::api::ProtocolError,
    > {
        Err(piko_hostd::api::ProtocolError::ObservationFailed(
            "mock recovery unavailable".into(),
        ))
    }

    async fn cancel_agent_input(
        &self,
        _session_id: &str,
        _agent_instance_id: &str,
        _input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, piko_hostd::api::ProtocolError> {
        Ok(piko_protocol::AgentInputCancelReceipt {
            input_id: _input_id.to_string(),
            request_id: _input_id.to_string(),
            session_id: _session_id.to_string(),
            agent_instance_id: _agent_instance_id.to_string(),
            accepted: false,
        })
    }

    async fn interrupt_agent(&self, _session_id: &str, _agent_instance_id: &str) -> bool {
        false
    }

    async fn has_active_session_run(&self, session_id: &str) -> bool {
        self.harness.session_has_active(session_id)
    }

    async fn list_agent_instances(
        &self,
        _session_id: &str,
    ) -> Option<Vec<piko_protocol::AgentInfo>> {
        None
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        let content = input.content.clone();
        let (publisher, subscription) = MockSessionPublisher::new(session_id.clone());
        self.publishers.lock().unwrap().push(Arc::clone(&publisher));

        // Sessions backed by a real schema-v4 journal get a durable commit;
        // ephemeral/in-memory-only test sessions skip persistence.
        let session_dir = self.session_dir.lock().unwrap().clone().unwrap_or_default();
        let store = SessionStore::new(session_dir);
        let mut committed_user: Option<String> = None;
        if store.load_projection().is_ok() {
            let now = chrono::Utc::now().timestamp_millis();
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            let committed = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    root_input_id: "input-mock".into(),
                    session_id: session_id.clone(),
                    source_turn_id: Some(input_id.clone()),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: message_id.clone(),
                    parent_message_id: None,
                    tree_parent_entry_id: None,
                    message: Message::User {
                        content: MessageContent::String(crate::support::content_text(&content)),
                        timestamp: Some(now),
                    },
                    committed_at: now,
                },
                "main",
            );
            if committed.is_ok() {
                committed_user = Some(message_id);
            }
        }

        let (completion_tx, completion_rx) = test_oneshot();
        let publisher_task = Arc::clone(&publisher);
        let publisher_agent = agent_instance_id.clone();
        let publisher_channel_input_id = input_id.clone();
        let publish_user_message = committed_user;
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(
                publisher_agent.clone(),
                "main",
                2,
                SessionEvent::InteractionResolved {
                    interaction_id: "running".into(),
                    status: piko_protocol::UserInteractionStatus::Submitted,
                },
            );
            if let Some(message_id) = publish_user_message {
                publisher_task.publish(
                    publisher_agent.clone(),
                    "main",
                    1,
                    SessionEvent::MessageCommitted {
                        transcript_seq: 1,
                        message_id,
                        source_turn_id: publisher_channel_input_id.clone(),
                        role: piko_protocol::MessageRole::User,
                    },
                );
            }
            publisher_task.publish(
                publisher_agent.clone(),
                "main",
                4,
                SessionEvent::InteractionResolved {
                    interaction_id: "completed".into(),
                    status: piko_protocol::UserInteractionStatus::Submitted,
                },
            );
            let barrier = publisher_task.cursor();
            let _ = completion_tx.send(AgentRunCompletion {
                input_id: publisher_channel_input_id,
                result: Ok(success_report(&publisher_agent)),
                observation_barrier: barrier,
            });
        });
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher,
        );
        Ok(piko_protocol::AgentInputReceipt {
            input_id,
            request_id: input.request_id,
            session_id,
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
