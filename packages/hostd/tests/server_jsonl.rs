use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hostd::api::{ApprovalDecision, Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::server::{HostServer, run_jsonl_server};
use hostd::session::JsonlSessionRepository;
use hostd::turn_runner::{TurnRunInput, TurnRunner};
use orchd::SessionSubscription;
use orchd::runtime::events::hub::{SessionOutputHub, merged_output_stream};
use piko_protocol::agent_runtime::{SessionEvent, SessionEventEnvelope, TaskSnapshot, TaskStatus};
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

struct SlowRunner;

#[async_trait]
impl TurnRunner for SlowRunner {
    async fn run_turn_subscription(
        &self,
        input: TurnRunInput,
    ) -> Result<SessionSubscription, hostd::api::ProtocolError> {
        let hub = Arc::new(SessionOutputHub::new(
            input.session_id.clone(),
            format!("slow_{}", uuid::Uuid::new_v4()),
            8,
        ));
        let cursor = hub.cursor();
        let subscription = merged_output_stream(hub.subscribe(), cursor.clone());
        let task_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let hub_task = Arc::clone(&hub);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = hub_task
                .publish_event(SessionEventEnvelope {
                    task_id: task_id.clone(),
                    agent_id: "main".into(),
                    task_seq: 0,
                    cursor: hub_task.cursor(),
                    event: SessionEvent::TaskChanged {
                        snapshot: TaskSnapshot {
                            session_id,
                            task_id,
                            agent_id: "main".into(),
                            parent_task_id: None,
                            status: TaskStatus::Created,
                            active_work: None,
                        },
                    },
                })
                .await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        std::mem::forget(hub);

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor,
            output: subscription,
        })
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
        use hostd::infra::storage::task_repository::SESSION_SCHEMA_VERSION;
        use hostd::infra::storage::{TaskRepository, TaskShardHeader};

        let Some(session_dir) = input.session_dir.clone() else {
            return Err(hostd::api::ProtocolError::InvalidCommand(
                "session_dir required for AssistantRunner".into(),
            ));
        };

        let hub = Arc::new(SessionOutputHub::new(
            input.session_id.clone(),
            format!("assistant_{}", uuid::Uuid::new_v4()),
            16,
        ));
        let cursor = hub.cursor();
        let subscription = merged_output_stream(hub.subscribe(), cursor.clone());
        let repository = TaskRepository::new(session_dir);
        let session_id = input.session_id.clone();
        let task_id = input.turn_id.clone();
        let turn_id = input.turn_id.clone();
        let prompt = input.prompt.clone();

        let _ = repository.create_task(TaskShardHeader {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            parent_task_id: None,
            created_at: 1,
        });
        let _ = repository.commit_message(orchd::integration::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: turn_id.clone(),
            task_seq: 1,
            message_id: "user-1".into(),
            parent_message_id: None,
            message: Message::User {
                content: MessageContent::String(prompt),
                timestamp: Some(1),
            },
            committed_at: 1,
        });
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
        let _ = repository.commit_message(orchd::integration::MessageCommit {
            session_id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: "agent-1".into(),
            work_id: turn_id.clone(),
            task_seq: 2,
            message_id: "assistant-1".into(),
            parent_message_id: Some("user-1".into()),
            message: assistant_message,
            committed_at: 3,
        });

        let hub_task = Arc::clone(&hub);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let publish = async |task_seq: u64, event: SessionEvent| {
                let _ = hub_task
                    .publish_event(SessionEventEnvelope {
                        task_id: task_id.clone(),
                        agent_id: "agent-1".into(),
                        task_seq,
                        cursor: hub_task.cursor(),
                        event,
                    })
                    .await;
            };

            publish(
                0,
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
            )
            .await;

            publish(
                1,
                SessionEvent::MessageCommitted {
                    message_id: "user-1".into(),
                    work_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            )
            .await;

            publish(
                2,
                SessionEvent::MessageCommitted {
                    message_id: "assistant-1".into(),
                    work_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            )
            .await;

            publish(
                3,
                SessionEvent::TaskChanged {
                    snapshot: TaskSnapshot {
                        session_id,
                        task_id: task_id.clone(),
                        agent_id: "agent-1".into(),
                        parent_task_id: None,
                        status: TaskStatus::Idle,
                        active_work: None,
                    },
                },
            )
            .await;
        });
        std::mem::forget(hub);

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor,
            output: subscription,
        })
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
        let hub = Arc::new(SessionOutputHub::new(
            input.session_id.clone(),
            format!("wait_{}", uuid::Uuid::new_v4()),
            8,
        ));
        let cursor = hub.cursor();
        let subscription = merged_output_stream(hub.subscribe(), cursor.clone());
        let started = self.started.clone();
        let finish = self.finish.clone();
        let task_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let hub_task = Arc::clone(&hub);

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = hub_task
                .publish_event(SessionEventEnvelope {
                    task_id: task_id.clone(),
                    agent_id: "main".into(),
                    task_seq: 0,
                    cursor: hub_task.cursor(),
                    event: SessionEvent::TaskChanged {
                        snapshot: TaskSnapshot {
                            session_id: session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: "main".into(),
                            parent_task_id: None,
                            status: TaskStatus::Created,
                            active_work: None,
                        },
                    },
                })
                .await;
            started.notify_one();
            finish.notified().await;
            let _ = hub_task
                .publish_event(SessionEventEnvelope {
                    task_id: task_id.clone(),
                    agent_id: "main".into(),
                    task_seq: 1,
                    cursor: hub_task.cursor(),
                    event: SessionEvent::TaskChanged {
                        snapshot: TaskSnapshot {
                            session_id,
                            task_id,
                            agent_id: "main".into(),
                            parent_task_id: None,
                            status: TaskStatus::Idle,
                            active_work: None,
                        },
                    },
                })
                .await;
        });
        std::mem::forget(hub);

        Ok(SessionSubscription {
            session_id: input.session_id,
            cursor,
            output: subscription,
        })
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
async fn in_memory_session_navigate_to_root_user_appends_leaf_and_resets_current_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage(repo);
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
