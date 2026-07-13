use std::sync::Arc;

use async_trait::async_trait;
use hostd::infra::storage::SessionStore;
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{Message, MessageContent, MessageRole};

use super::{MockSessionPublisher, successful_turn_run};

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

#[async_trait]
impl TurnRunner for MockTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let work_id = input.work_id.clone();
        let source_turn_id = input.turn_id.clone();
        let task_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let mut committed_user: Option<String> = None;

        // Sessions backed by a real on-disk store (schema v3) get a durable
        // commit; ephemeral/in-memory-only test sessions skip persistence.
        let store = SessionStore::new(&input.session_dir);
        if store.load_manifest().is_ok() {
            let now = chrono::Utc::now().timestamp_millis();
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            let committed = store.commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(source_turn_id.clone()),
                    execution_id: task_id.clone(),
                    agent_instance_id: task_id.clone(),
                    message_id: message_id.clone(),
                    parent_message_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
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

        let barrier_seq = if committed_user.is_some() { 3 } else { 2 };
        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;

            publisher_task.publish(
                task_id.clone(),
                "main",
                2,
                SessionEvent::InteractionResolved {
                    resolution: serde_json::json!({"marker": "running"}),
                },
            );

            if let Some(message_id) = committed_user {
                publisher_task.publish(
                    task_id.clone(),
                    "main",
                    1,
                    SessionEvent::MessageCommitted {
                        message_id,
                        work_id: work_id.clone(),
                        role: MessageRole::User,
                    },
                );
            }

            publisher_task.publish(
                task_id.clone(),
                "main",
                4,
                SessionEvent::InteractionResolved {
                    resolution: serde_json::json!({"marker": "completed"}),
                },
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "root",
            barrier_seq,
            std::time::Duration::ZERO,
        ))
    }
}
