use std::sync::Arc;

use async_trait::async_trait;
use hostd::domain::turns::{TurnRunInput, TurnRunner};
use orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{
    ExecutionObservationSnapshot, ExecutionStatus, Message, MessageContent, MessageRole,
};

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
        let source_turn_id = input.turn_id.clone();
        let task_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let mut committed_user: Option<String> = None;

        if let Some(sink) = input.persist_sink.as_ref() {
            let now = chrono::Utc::now().timestamp_millis();
            sink.ensure_task_shard(orchd_api::TaskShardEnsure {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                agent_instance_id: None,
                parent_task_id: None,
                created_at: now,
            })
            .await
            .expect("mock shard ensure should succeed");
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            sink.commit_message(orchd_api::MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                agent_instance_id: None,
                work_id: work_id.clone(),
                task_seq: 1,
                message_id: message_id.clone(),
                parent_message_id: None,
                message: Message::User {
                    content: MessageContent::String(prompt),
                    timestamp: Some(now),
                },
                committed_at: now,
            })
            .await
            .expect("mock message commit should succeed");
            committed_user = Some(message_id);
        }

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;

            publisher_task.publish(
                task_id.clone(),
                "main",
                2,
                SessionEvent::ExecutionChanged {
                    snapshot: ExecutionObservationSnapshot {
                        session_id: session_id.clone(),
                        turn_id: source_turn_id.clone(),
                        execution_id: task_id.clone(),
                        agent_instance_id: "root".into(),
                        agent_id: "main".into(),
                        status: ExecutionStatus::Running,
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
                4,
                SessionEvent::ExecutionChanged {
                    snapshot: ExecutionObservationSnapshot {
                        session_id,
                        turn_id: source_turn_id,
                        execution_id: task_id,
                        agent_instance_id: "root".into(),
                        agent_id: "main".into(),
                        status: ExecutionStatus::Succeeded,
                    },
                },
            );
        });

        Ok(subscription)
    }
}
