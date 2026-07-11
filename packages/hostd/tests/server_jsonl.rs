mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hostd::api::{ApprovalDecision, Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::domain::turns::{TurnRunInput, TurnRunner};
use hostd::infra::storage::JsonlSessionRepository;
use hostd::protocol::{HostServer, run_jsonl_server};
use orchd_api::SessionSubscription;
use piko_protocol::agent_runtime::{SessionEvent, TaskSnapshot, TaskStatus};
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{MockSessionPublisher, MockTurnRunner};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

struct SlowRunner;

struct MissingProjectionRunner;

#[async_trait]
impl TurnRunner for MissingProjectionRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        tokio::spawn(async move {
            publisher.publish(
                input.work_id.clone(),
                "main",
                1,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: input.session_id.clone(),
                        task_id: input.work_id.clone(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Running,
                        active_work: Some(piko_protocol::agent_runtime::WorkSnapshot {
                            work_id: input.work_id.clone(),
                            status: piko_protocol::agent_runtime::WorkStatus::Running,
                            source_turn_id: Some(input.turn_id.clone()),
                        }),
                    },
                },
            );
            publisher.publish(
                input.work_id.clone(),
                "main",
                2,
                SessionEvent::MessageCommitted {
                    message_id: "missing-message".into(),
                    work_id: input.work_id,
                    role: MessageRole::Assistant,
                },
            );
        });
        Ok(subscription)
    }
}

struct HalfFinalizedRunner;

#[async_trait]
impl TurnRunner for HalfFinalizedRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        tokio::spawn(async move {
            publisher.publish(
                input.work_id.clone(),
                "main",
                1,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: input.session_id.clone(),
                        task_id: input.work_id.clone(),
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Running,
                        active_work: Some(piko_protocol::agent_runtime::WorkSnapshot {
                            work_id: input.work_id.clone(),
                            status: piko_protocol::agent_runtime::WorkStatus::Running,
                            source_turn_id: Some(input.turn_id.clone()),
                        }),
                    },
                },
            );
            publisher.publish(
                input.work_id.clone(),
                "main",
                2,
                SessionEvent::WorkChanged {
                    snapshot: piko_protocol::agent_runtime::WorkSnapshot {
                        work_id: input.work_id,
                        status: piko_protocol::agent_runtime::WorkStatus::Succeeded,
                        source_turn_id: Some(input.turn_id),
                    },
                },
            );
        });
        Ok(subscription)
    }
}

#[async_trait]
impl TurnRunner for SlowRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let task_id = input.work_id.clone();
        let work_id = input.work_id.clone();
        let source_turn_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let publisher_task = Arc::clone(&publisher);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(
                task_id.clone(),
                "main",
                0,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id,
                        agent_id: "main".into(),
                        parent_task_id: None,
                        status: TaskStatus::Running,
                        active_work: Some(piko_protocol::agent_runtime::WorkSnapshot {
                            work_id,
                            status: piko_protocol::agent_runtime::WorkStatus::Running,
                            source_turn_id: Some(source_turn_id),
                        }),
                    },
                },
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        Ok(subscription)
    }
}

#[tokio::test]
async fn command_catalog_get_returns_slash_commands() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::CommandCatalogGet {
            command_id: "catalog-1".into(),
        })
        .await;

    let [
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::CommandCatalogListed { commands, .. }),
            ..
        },
    ] = events.as_slice()
    else {
        panic!("expected command catalog event, got {events:?}");
    };
    assert!(commands.iter().any(|command| command.slash_name == "/help"));
}

struct AssistantRunner;

#[async_trait]
impl TurnRunner for AssistantRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let Some(sink) = input.persist_sink.clone() else {
            return Err(hostd::api::ProtocolError::InvalidCommand(
                "persist_sink required for AssistantRunner".into(),
            ));
        };

        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let task_id = input.work_id.clone();
        let work_id = input.work_id.clone();
        let source_turn_id = input.turn_id.clone();
        let turn_id = input.work_id.clone();
        let prompt = input.prompt.clone();

        sink.commit_task_event(orchd_api::TaskEventCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            task_seq: 1,
            event: piko_protocol::TaskEvent::Created {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "agent-1".into(),
                parent_task_id: None,
                source_agent_id: None,
                prompt: prompt.clone(),
                work_id: turn_id.clone(),
                timestamp: 1,
            },
            committed_at: 1,
        })
        .await
        .unwrap();
        sink.commit_message(orchd_api::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: turn_id.clone(),
            task_seq: 2,
            message_id: "user-1".into(),
            parent_message_id: None,
            message: Message::User {
                content: MessageContent::String(prompt),
                timestamp: Some(1),
            },
            committed_at: 1,
        })
        .await
        .unwrap();
        let assistant_message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: "world".into(),
            }],
            api: "test".into(),
            provider: "test-provider".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(3),
        };
        sink.commit_message(orchd_api::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: turn_id.clone(),
            task_seq: 3,
            message_id: "assistant-1".into(),
            parent_message_id: Some("user-1".into()),
            message: assistant_message,
            committed_at: 3,
        })
        .await
        .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                1,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                        agent_id: "agent-1".into(),
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

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                2,
                SessionEvent::MessageCommitted {
                    message_id: "user-1".into(),
                    work_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                3,
                SessionEvent::MessageCommitted {
                    message_id: "assistant-1".into(),
                    work_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                3,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id,
                        agent_id: "agent-1".into(),
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

#[derive(Default)]
struct ReuseRootTurnRunner {
    turn_count: std::sync::atomic::AtomicU32,
    root_task_id: std::sync::Mutex<Option<String>>,
}

#[async_trait]
impl TurnRunner for ReuseRootTurnRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let Some(sink) = input.persist_sink.clone() else {
            return Err(hostd::api::ProtocolError::InvalidCommand(
                "persist_sink required for ReuseRootTurnRunner".into(),
            ));
        };

        let turn = self
            .turn_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let task_id = if turn == 0 {
            let id = input.work_id.clone();
            *self.root_task_id.lock().unwrap() = Some(id.clone());
            id
        } else {
            self.root_task_id
                .lock()
                .unwrap()
                .clone()
                .expect("root task id")
        };
        let work_id = input.work_id.clone();
        let source_turn_id = input.turn_id.clone();
        let prompt = input.prompt.clone();

        if turn == 0 {
            sink.commit_task_event(orchd_api::TaskEventCommit {
                session_id: session_id.clone(),
                task_id: task_id.clone(),
                agent_id: "agent-1".into(),
                task_seq: 1,
                event: piko_protocol::TaskEvent::Created {
                    session_id: session_id.clone(),
                    task_id: task_id.clone(),
                    agent_id: "agent-1".into(),
                    parent_task_id: None,
                    source_agent_id: None,
                    prompt: prompt.clone(),
                    work_id: work_id.clone(),
                    timestamp: 1,
                },
                committed_at: 1,
            })
            .await
            .unwrap();
        }

        let user_message_id: String = if turn == 0 {
            "user-1".into()
        } else {
            "user-2".into()
        };
        let user_task_seq = if turn == 0 { 2 } else { 4 };
        sink.commit_message(orchd_api::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: work_id.clone(),
            task_seq: user_task_seq,
            message_id: user_message_id.clone(),
            parent_message_id: if turn == 0 {
                None
            } else {
                Some("assistant-1".into())
            },
            message: Message::User {
                content: MessageContent::String(prompt),
                timestamp: Some(1),
            },
            committed_at: 1,
        })
        .await
        .unwrap();

        let assistant_message_id: String = if turn == 0 {
            "assistant-1".into()
        } else {
            "assistant-2".into()
        };
        let assistant_task_seq = if turn == 0 { 3 } else { 5 };
        let assistant_message = Message::Assistant {
            content: vec![ContentBlock::Text {
                text: if turn == 0 {
                    "world".into()
                } else {
                    "again".into()
                },
            }],
            api: "test".into(),
            provider: "test-provider".into(),
            model: "test-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: Some(3),
        };
        sink.commit_message(orchd_api::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: work_id.clone(),
            task_seq: assistant_task_seq,
            message_id: assistant_message_id.clone(),
            parent_message_id: Some(user_message_id.clone()),
            message: assistant_message,
            committed_at: 3,
        })
        .await
        .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            if turn == 0 {
                publisher_task.publish(
                    task_id.clone(),
                    "agent-1",
                    1,
                    SessionEvent::TaskChanged {
                        snapshot: TaskSnapshot {
                            session_id: session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: "agent-1".into(),
                            parent_task_id: None,
                            status: TaskStatus::Created,
                            active_work: None,
                        },
                    },
                );
            }

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                user_task_seq.saturating_sub(1),
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id: session_id.clone(),
                        task_id: task_id.clone(),
                        agent_id: "agent-1".into(),
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

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                user_task_seq,
                SessionEvent::MessageCommitted {
                    message_id: user_message_id,
                    work_id: work_id.clone(),
                    role: MessageRole::User,
                },
            );

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                assistant_task_seq,
                SessionEvent::MessageCommitted {
                    message_id: assistant_message_id,
                    work_id: work_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                task_id.clone(),
                "agent-1",
                assistant_task_seq + 1,
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
                "agent-1",
                assistant_task_seq + 2,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id,
                        agent_id: "agent-1".into(),
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

struct WaitingApprovalRunner {
    started: Arc<Notify>,
    finish: Arc<Notify>,
}

#[async_trait]
impl TurnRunner for WaitingApprovalRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let started = self.started.clone();
        let finish = self.finish.clone();
        let task_id = input.work_id.clone();
        let work_id = input.work_id.clone();
        let source_turn_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let publisher_task = Arc::clone(&publisher);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(
                task_id.clone(),
                "main",
                0,
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
            started.notify_one();
            finish.notified().await;
            publisher_task.publish(
                task_id.clone(),
                "main",
                1,
                SessionEvent::WorkChanged {
                    snapshot: piko_protocol::agent_runtime::WorkSnapshot {
                        work_id,
                        status: piko_protocol::agent_runtime::WorkStatus::Succeeded,
                        source_turn_id: Some(source_turn_id),
                    },
                },
            );
            publisher_task.publish(
                task_id.clone(),
                "main",
                2,
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

    async fn respond_approval(
        &self,
        _approval_id: &str,
        _decision: ApprovalDecision,
    ) -> Result<bool, hostd::api::ProtocolError> {
        Ok(true)
    }
}

#[tokio::test]
async fn turn_submit_streams_started_before_runner_finishes() {
    let server = HostServer::with_turn_runner(Arc::new(SlowRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let mut events = server.handle_command_stream(Command::TurnSubmit {
        command_id: "submit".into(),
        session_id,
        text: "hello".into(),
    });

    let started = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        started,
        Event::TurnLifecycle(hostd::api::TurnEvent::Started { .. })
    ));
}

#[tokio::test]
async fn approval_response_is_not_blocked_by_active_turn() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
    }));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let mut events = server.handle_command_stream(Command::TurnSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        text: "hello".into(),
    });

    let first = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        first,
        Event::TurnLifecycle(hostd::api::TurnEvent::Started { .. })
    ));
    tokio::time::timeout(Duration::from_millis(50), started.notified())
        .await
        .unwrap();

    let approval_events = tokio::time::timeout(
        Duration::from_millis(50),
        server.handle_command(Command::ApprovalRespond {
            command_id: "approval".into(),
            session_id,
            approval_id: "approval-1".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        }),
    )
    .await
    .expect("approval response should not wait for the active turn to finish");

    assert!(matches!(
        approval_events.first(),
        Some(Event::Approval(hostd::api::ApprovalEvent::Resolved { .. }))
    ));

    finish.notify_one();
}

#[tokio::test]
async fn create_session_returns_session_created() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::SessionCreate {
            command_id: "cmd-1".into(),
            cwd: "/tmp/project".into(),
        })
        .await;

    assert!(!events.is_empty());
    assert!(matches!(
        events[0],
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { .. }),
            ..
        }
    ));
}

async fn assert_turn_fails_without_completion(runner: Arc<dyn TurnRunner>) {
    let temp = tempfile::tempdir().unwrap();
    let server =
        HostServer::with_storage_and_runner(JsonlSessionRepository::new(temp.path()), runner);
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };
    let events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id,
            text: "hello".into(),
        })
        .await;
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Failed { .. })
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
    )));
}

#[tokio::test]
async fn missing_committed_projection_fails_turn() {
    assert_turn_fails_without_completion(Arc::new(MissingProjectionRunner)).await;
}

#[tokio::test]
async fn work_success_without_task_stable_fails_turn() {
    assert_turn_fails_without_completion(Arc::new(HalfFinalizedRunner)).await;
}

#[tokio::test]
async fn turn_submit_persists_completed_assistant_as_session_entry() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(AssistantRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let turn_events = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;
    let committed = turn_events
        .iter()
        .filter_map(|event| match event {
            Event::TranscriptCommitted(committed) => Some(committed),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(committed.len(), 2);
    assert_eq!(committed[0].task_seq, 2);
    assert!(matches!(committed[0].message, Message::User { .. }));
    assert_eq!(committed[1].task_seq, 3);
    assert!(matches!(committed[1].message, Message::Assistant { .. }));

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;

    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }),
        ..
    } = &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    assert_eq!(snapshot.entries.len(), 2);
    assert!(matches!(
        &snapshot.entries[0],
        hostd::api::SessionTreeEntry::Message(entry)
            if matches!(entry.message, hostd::api::Message::User { .. })
    ));
    assert!(matches!(
        &snapshot.entries[1],
        hostd::api::SessionTreeEntry::Message(entry)
            if entry.id == "assistant-1"
                && matches!(entry.message, hostd::api::Message::Assistant { .. })
    ));
}

#[tokio::test]
async fn turn_submit_reuses_session_sink_across_turns() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server =
        HostServer::with_storage_and_runner(repo, Arc::new(ReuseRootTurnRunner::default()));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    for (command_id, text) in [("submit-1", "hello"), ("submit-2", "follow up")] {
        let turn_events = server
            .handle_command(Command::TurnSubmit {
                command_id: command_id.into(),
                session_id: session_id.clone(),
                text: text.into(),
            })
            .await;
        for event in &turn_events {
            if let Event::CommandResponse {
                result: Err(err), ..
            } = event
            {
                panic!("turn {command_id} failed: {err}");
            }
        }
        assert!(
            turn_events.iter().any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
            )),
            "turn {command_id} should complete"
        );
    }

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }),
        ..
    } = &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    assert_eq!(snapshot.entries.len(), 4);
}

#[tokio::test]
async fn in_memory_session_navigate_to_root_user_appends_leaf_and_resets_current_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage_and_runner(repo, Arc::new(MockTurnRunner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let _ = server
        .handle_command(Command::TurnSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            text: "hello".into(),
        })
        .await;

    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::StateSnapshot { snapshot, .. }),
        ..
    } = &snapshot[0]
    else {
        panic!("expected state snapshot");
    };
    let root_user_id = snapshot.entries[0].id().to_string();

    let navigated = server
        .handle_command(Command::SessionNavigate {
            command_id: "navigate".into(),
            session_id: session_id.clone(),
            entry_id: root_user_id.clone(),
            summarize: false,
            custom_instructions: None,
        })
        .await;

    assert!(matches!(
        &navigated[0],
        Event::CommandResponse { result: Ok(hostd::api::CommandResult::SessionNavigated {
            new_leaf_id: None,
            selected_entry_id,
            editor_text: Some(text),
            ..
        }), .. } if selected_entry_id == &root_user_id && text == "hello"
    ));
    let Event::CommandResponse {
        result: Ok(hostd::api::CommandResult::SessionOpened { snapshot, .. }),
        ..
    } = &navigated[1]
    else {
        panic!("expected session opened");
    };
    assert_eq!(snapshot.current_leaf_id, None);
    assert!(matches!(
        snapshot.entries.last(),
        Some(SessionTreeEntry::Leaf(leaf)) if leaf.target_id.is_none()
    ));
}

#[tokio::test]
async fn jsonl_server_round_trips_events() {
    let input = serde_json::to_string(&Command::SessionCreate {
        command_id: "create".into(),
        cwd: "/tmp/project".into(),
    })
    .unwrap()
        + "\n";
    let (mut read_out, write_out) = tokio::io::duplex(4096);
    run_jsonl_server(
        std::io::Cursor::new(input.into_bytes()),
        write_out,
        HostServer::new(),
    )
    .await
    .unwrap();

    let mut output = String::new();
    read_out.read_to_string(&mut output).await.unwrap();
    let mut lines = output.lines();
    let ack = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(
        ack,
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::Empty),
            ..
        }
    ));

    let event = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(
        event,
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { .. }),
            ..
        }
    ));
}

#[tokio::test]
async fn jsonl_server_reads_next_command_while_turn_is_running() {
    let started = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let server = HostServer::with_turn_runner(Arc::new(WaitingApprovalRunner {
        started: started.clone(),
        finish: finish.clone(),
    }));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = match &created[0] {
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
            ..
        } => session_id.clone(),
        other => panic!("expected session_created, got {other:?}"),
    };

    let (client_in, server_in) = tokio::io::duplex(4096);
    let (server_out, client_out) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        run_jsonl_server(BufReader::new(server_in), server_out, server)
            .await
            .unwrap();
    });
    let mut writer = client_in;
    let mut reader = BufReader::new(client_out);

    let submit = serde_json::to_string(&Command::TurnSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        text: "hello".into(),
    })
    .unwrap();
    writer.write_all(submit.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();

    let mut line = String::new();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_submit ack should arrive")
        .unwrap();
    let ack = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        ack,
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::Empty),
            ..
        }
    ));

    line.clear();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_started should arrive")
        .unwrap();
    let event = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        event,
        Event::TurnLifecycle(hostd::api::TurnEvent::Started { .. })
    ));

    let approval = serde_json::to_string(&Command::ApprovalRespond {
        command_id: "approval".into(),
        session_id,
        approval_id: "approval-1".into(),
        decision: ApprovalDecision::Accept,
        note: None,
    })
    .unwrap();
    writer.write_all(approval.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut saw_approval_ack = false;
    let mut saw_approval_event = false;
    for _ in 0..10 {
        line.clear();
        tokio::time::timeout(Duration::from_millis(500), reader.read_line(&mut line))
            .await
            .expect("approval output should not wait for turn completion")
            .unwrap();
        let value = serde_json::from_str::<serde_json::Value>(line.trim()).unwrap();
        if value.get("kind").and_then(|v| v.as_str()) == Some("command_response")
            && value.get("command_id").and_then(|v| v.as_str()) == Some("approval")
        {
            saw_approval_ack = true;
        }
        if value.get("kind").and_then(|v| v.as_str()) == Some("approval")
            && value.get("type").and_then(|v| v.as_str()) == Some("resolved")
        {
            saw_approval_event = true;
        }
        if saw_approval_ack && saw_approval_event {
            break;
        }
    }
    assert!(saw_approval_ack);
    assert!(saw_approval_event);

    finish.notify_one();
    drop(writer);
    server_task.await.unwrap();
}

#[tokio::test]
async fn test_config_update_returns_config_changed_event() {
    let server = HostServer::new();
    let events = server
        .handle_command(Command::ConfigUpdate {
            command_id: "cfg-1".into(),
            patch: serde_json::json!({}),
        })
        .await;

    let mut found = false;
    for event in events {
        if let Event::Model(hostd::api::ModelEvent::ConfigChanged { .. }) = event {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "expected ModelEvent::ConfigChanged in response to ConfigUpdate"
    );
}
