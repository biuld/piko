#[path = "support/mock_turn_runner.rs"]
mod mock_turn_runner;
mod support;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hostd::api::{ApprovalDecision, Command, Message, ServerMessage as Event, SessionTreeEntry};
use hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use hostd::protocol::{HostServer, run_jsonl_server};
use mock_turn_runner::MockTurnRunner;
use piko_protocol::agent_runtime::SessionEvent;
use piko_protocol::{ContentBlock, MessageContent, MessageRole};
use support::{
    MockSessionPublisher, execution_running, execution_succeeded, success_report,
    successful_turn_run,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

struct SlowRunner;

#[async_trait]
impl TurnRunner for SlowRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let agent_instance_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(
                agent_instance_id.clone(),
                "main",
                0,
                execution_running(
                    session_id,
                    agent_instance_id.clone(),
                    agent_instance_id,
                    "main",
                ),
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "root",
            1,
            Duration::from_millis(200),
        ))
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
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let store = SessionStore::new(&input.session_dir);

        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = input.turn_id.clone();
        let turn_id = input.turn_id.clone();
        let prompt = input.prompt.clone();

        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "user-1".into(),
                    parent_message_id: None,
                    message: Message::User {
                        content: MessageContent::String(prompt),
                        timestamp: Some(1),
                    },
                    committed_at: 1,
                },
                "agent-1",
            )
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
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: "assistant-1".into(),
                    parent_message_id: Some("user-1".into()),
                    message: assistant_message,
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                0,
                execution_running(
                    session_id.clone(),
                    turn_id.clone(),
                    agent_instance_id.clone(),
                    "agent-1",
                ),
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                1,
                SessionEvent::MessageCommitted {
                    message_id: "user-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                SessionEvent::MessageCommitted {
                    message_id: "assistant-1".into(),
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                2,
                execution_succeeded(session_id, turn_id, agent_instance_id, "agent-1"),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "root",
            4,
            Duration::ZERO,
        ))
    }
}

#[derive(Default)]
struct ReuseRootTurnRunner {
    turn_count: std::sync::atomic::AtomicU32,
    root_agent_instance_id: std::sync::Mutex<Option<String>>,
}

#[async_trait]
impl TurnRunner for ReuseRootTurnRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let store = SessionStore::new(&input.session_dir);

        let turn = self
            .turn_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let session_id = input.session_id.clone();
        let agent_instance_id = if turn == 0 {
            let id = input.turn_id.clone();
            *self.root_agent_instance_id.lock().unwrap() = Some(id.clone());
            id
        } else {
            self.root_agent_instance_id
                .lock()
                .unwrap()
                .clone()
                .expect("root agent instance id")
        };
        let turn_id = input.turn_id.clone();
        let prompt = input.prompt.clone();

        let user_message_id: String = if turn == 0 {
            "user-1".into()
        } else {
            "user-2".into()
        };
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
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
                },
                "agent-1",
            )
            .unwrap();

        let assistant_message_id: String = if turn == 0 {
            "assistant-1".into()
        } else {
            "assistant-2".into()
        };
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
        store
            .commit_message(
                piko_protocol::execution::MessageCommit {
                    session_id: session_id.clone(),
                    source_turn_id: Some(turn_id.clone()),
                    execution_id: agent_instance_id.clone(),
                    agent_instance_id: agent_instance_id.clone(),
                    message_id: assistant_message_id.clone(),
                    parent_message_id: Some(user_message_id.clone()),
                    message: assistant_message,
                    committed_at: 3,
                },
                "agent-1",
            )
            .unwrap();

        let user_task_seq: u64 = if turn == 0 { 1 } else { 3 };
        let assistant_task_seq: u64 = if turn == 0 { 2 } else { 4 };
        let publisher_task = Arc::clone(&publisher);
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            if turn == 0 {
                publisher_task.publish(
                    agent_instance_id.clone(),
                    "agent-1",
                    1,
                    execution_running(
                        session_id.clone(),
                        turn_id.clone(),
                        agent_instance_id.clone(),
                        "agent-1",
                    ),
                );
            }

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                user_task_seq,
                SessionEvent::MessageCommitted {
                    message_id: user_message_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::User,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                assistant_task_seq,
                SessionEvent::MessageCommitted {
                    message_id: assistant_message_id,
                    source_turn_id: turn_id.clone(),
                    role: MessageRole::Assistant,
                },
            );

            publisher_task.publish(
                agent_instance_id.clone(),
                "agent-1",
                assistant_task_seq + 1,
                execution_succeeded(session_id, turn_id, agent_instance_id, "agent-1"),
            );
        });

        Ok(successful_turn_run(
            subscription,
            input.session_id,
            input.turn_id,
            "root",
            if turn == 0 { 4 } else { 3 },
            Duration::ZERO,
        ))
    }
}

struct WaitingApprovalRunner {
    started: Arc<Notify>,
    finish: Arc<Notify>,
}

#[async_trait]
impl TurnRunner for WaitingApprovalRunner {
    async fn run_turn(
        &self,
        input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        let (publisher, subscription) = MockSessionPublisher::new(input.session_id.clone());
        let started = self.started.clone();
        let finish = self.finish.clone();
        let agent_instance_id = input.turn_id.clone();
        let session_id = input.session_id.clone();
        let publisher_task = Arc::clone(&publisher);
        let barrier = piko_protocol::agent_runtime::SessionCursor {
            epoch: subscription.cursor.epoch.clone(),
            seq: 2,
        };
        let (completion_tx, completion) = tokio::sync::oneshot::channel();
        let completion_session_id = input.session_id.clone();
        let completion_turn_id = input.turn_id.clone();

        tokio::spawn(async move {
            tokio::task::yield_now().await;
            publisher_task.publish(
                agent_instance_id.clone(),
                "main",
                0,
                execution_running(
                    session_id.clone(),
                    agent_instance_id.clone(),
                    agent_instance_id.clone(),
                    "main",
                ),
            );
            started.notify_one();
            finish.notified().await;
            publisher_task.publish(
                agent_instance_id.clone(),
                "main",
                1,
                execution_succeeded(
                    session_id,
                    agent_instance_id.clone(),
                    agent_instance_id,
                    "main",
                ),
            );
            let _ = completion_tx.send(hostd::ports::TurnRunCompletion {
                session_id: completion_session_id,
                turn_id: completion_turn_id,
                root_agent_instance_id: "root".into(),
                result: Ok(success_report("root")),
                observation_barrier: barrier,
            });
        });

        Ok(TurnRunHandle {
            session_id: input.session_id,
            turn_id: input.turn_id,
            root_agent_instance_id: "root".into(),
            observation: subscription,
            completion,
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
async fn root_chat_streams_started_before_runner_finishes() {
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

    let mut events = server.handle_command_stream(Command::ChatSubmit {
        command_id: "submit".into(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        session_id: session_id.clone(),
        text: "hello".into(),
    });

    let accepted = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        accepted,
        Event::CommandResponse {
            command_id,
            result: Ok(hostd::api::CommandResult::Empty),
        } if command_id == "submit"
    ));

    let started = events.recv().await.unwrap();
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

    let mut events = server.handle_command_stream(Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        text: "hello".into(),
    });

    let accepted = tokio::time::timeout(Duration::from_millis(50), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        accepted,
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::Empty),
            ..
        }
    ));
    let started_event = events.recv().await.unwrap();
    assert!(matches!(
        started_event,
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

    assert!(
        approval_events.iter().any(|event| matches!(
            event,
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::Empty),
                ..
            }
        )),
        "approval should return business Empty; events={approval_events:?}"
    );
    assert!(
        approval_events.iter().any(|event| matches!(
            event,
            Event::Approval(hostd::api::ApprovalEvent::Resolved { .. })
        )),
        "approval should emit resolved; events={approval_events:?}"
    );

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
async fn root_chat_persists_completed_assistant_as_session_entry() {
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
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
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
    assert_eq!(committed[0].transcript_seq, 1);
    assert!(matches!(committed[0].message, Message::User { .. }));
    assert_eq!(committed[1].transcript_seq, 2);
    assert!(matches!(committed[1].message, Message::Assistant { .. }));
    let final_message_index = turn_events
        .iter()
        .rposition(|event| matches!(event, Event::TranscriptCommitted(_)))
        .unwrap();
    let completed_index = turn_events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::TurnLifecycle(hostd::api::TurnEvent::Completed { .. })
            )
        })
        .unwrap();
    assert!(
        final_message_index < completed_index,
        "completion barrier must project final transcript before TurnCompleted"
    );

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;

    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
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
async fn root_chat_reuses_session_sink_across_turns() {
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
            .handle_command(Command::ChatSubmit {
                command_id: command_id.into(),
                session_id: session_id.clone(),
                target_agent_instance_id: format!("agent_{session_id}_root"),
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
                Event::TurnLifecycle(piko_protocol::TurnEvent::Started { .. })
            )),
            "turn {command_id} must emit TurnStarted for TUI spinner; events={turn_events:?}"
        );
        assert!(
            turn_events.iter().any(|event| matches!(
                event,
                Event::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
            )),
            "turn {command_id} should complete"
        );
    }

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
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
        .handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: format!("agent_{session_id}_root"),
            text: "hello".into(),
        })
        .await;

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let snapshot = refresh
        .iter()
        .find_map(|event| match event {
            Event::SessionReconciled(reconciled) => Some(&reconciled.snapshot),
            _ => None,
        })
        .expect("expected reconciled snapshot");
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
    let Event::SessionReconciled(reconciled) = &navigated[1] else {
        panic!("expected session reconciled");
    };
    assert_eq!(reconciled.snapshot.current_leaf_id, None);
    assert!(matches!(
        reconciled.snapshot.entries.last(),
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
    let event = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(
        event,
        Event::CommandResponse {
            result: Ok(hostd::api::CommandResult::SessionCreated { .. }),
            ..
        }
    ));
    let reconciled = serde_json::from_str::<Event>(lines.next().unwrap()).unwrap();
    assert!(matches!(reconciled, Event::SessionReconciled(_)));
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

    let submit = serde_json::to_string(&Command::ChatSubmit {
        command_id: "submit".into(),
        session_id: session_id.clone(),
        target_agent_instance_id: format!("agent_{session_id}_root"),
        text: "hello".into(),
    })
    .unwrap();
    writer.write_all(submit.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();

    let mut line = String::new();
    tokio::time::timeout(Duration::from_millis(100), reader.read_line(&mut line))
        .await
        .expect("turn_started should arrive")
        .unwrap();
    let event = serde_json::from_str::<Event>(line.trim()).unwrap();
    assert!(matches!(
        event,
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
