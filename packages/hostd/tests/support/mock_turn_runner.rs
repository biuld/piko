use std::sync::Arc;

use async_trait::async_trait;
use hostd::domain::turns::{TurnRunInput, TurnRunner};
use orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::{SessionEvent, TaskSnapshot, TaskStatus};
use piko_protocol::{Message, MessageContent, MessageRole};

use super::MockSessionPublisher;

#[derive(Debug, Clone, Default)]
pub struct MockTurnRunner;

#[async_trait]
impl TurnRunner for MockTurnRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let work_id = input.work_id.clone();
        let task_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let mut committed_user: Option<String> = None;

        if let Some(path) = input.session_dir.as_ref() {
            use hostd::infra::storage::TaskShardHeader;
            use hostd::infra::storage::task_repository::SESSION_SCHEMA_VERSION;
            let repository = hostd::infra::storage::TaskRepository::new(path);
            let now = chrono::Utc::now().timestamp_millis();
            let header = TaskShardHeader {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                parent_task_id: None,
                created_at: now,
            };
            let _ = repository.create_task(header);
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            let _ = repository.commit_message(orchd_api::MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                work_id: work_id.clone(),
                task_seq: 1,
                message_id: message_id.clone(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String(prompt),
                    timestamp: Some(now),
                },
                committed_at: now,
            });
            committed_user = Some(message_id);
        }

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;

            publisher_task.publish(
                task_id.clone(),
                "main",
                1,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Created,
                        active_work: None,
                    },
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
                1,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id,
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Idle,
                        active_work: None,
                    },
                },
            );
        });

        Ok(subscription)
    }
}
