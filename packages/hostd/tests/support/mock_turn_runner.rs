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
        let source_turn_id = input.turn_id.clone();
        let task_id = input.work_id.clone();
        let prompt = input.prompt.clone();
        let mut committed_user: Option<String> = None;

        if let Some(sink) = input.persist_sink.as_ref() {
            let now = chrono::Utc::now().timestamp_millis();
            let created = piko_protocol::TaskEvent::Created {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                parent_task_id: None,
                source_agent_id: None,
                prompt: prompt.clone(),
                work_id: work_id.clone(),
                timestamp: now,
            };
            sink.commit_task_event(orchd_api::TaskEventCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                task_seq: 1,
                event: created,
                committed_at: now,
            })
            .await
            .expect("mock task commit should succeed");
            let message_id = format!("msg_{}", uuid::Uuid::new_v4());
            sink.commit_message(orchd_api::MessageCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "main".into(),
                work_id: work_id.clone(),
                task_seq: 2,
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
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Running,
                        active_work: Some(piko_protocol::agent_runtime::WorkSnapshot {
                            work_id: work_id.clone(),
                            status: piko_protocol::agent_runtime::WorkStatus::Running,
                            source_turn_id: Some(source_turn_id.clone()),
                        }),
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
                3,
                SessionEvent::WorkChanged {
                    snapshot: piko_protocol::agent_runtime::WorkSnapshot {
                        work_id: work_id.clone(),
                        status: piko_protocol::agent_runtime::WorkStatus::Succeeded,
                        source_turn_id: Some(source_turn_id),
                    },
                },
            );

            publisher_task.publish(
                task_id.clone(),
                "main",
                4,
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
