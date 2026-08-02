//! F-07 acceptance: an unanswered tool-approval request expires after the
//! configured deadline, resolves fail-closed, and late responses are ignored.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_core::Stream;
use piko_hostd::adapters::OrchAgentRunRunner;
use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::domain::config::ApprovalSettings;
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use piko_protocol::Model;
use piko_protocol::messages::Message;
use piko_protocol::model::ModelRunSettings;
use piko_protocol::{ApprovalDecision, ApprovalEvent};
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

/// Step 0 emits a `bash` tool call (requires approval); every later step is a
/// plain text reply so the turn terminates after the expiry resolution.
struct ScriptedBashGateway {
    step: AtomicUsize,
}

impl ScriptedBashGateway {
    fn new() -> Self {
        Self {
            step: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmGateway for ScriptedBashGateway {
    async fn chat_stream(
        &self,
        _request: GatewayRequest,
        _cancel: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        if step == 0 {
            Ok(Box::pin(iter(vec![
                GatewayEvent::ToolCallChunk {
                    id: "call-bash".into(),
                    name: "bash".into(),
                    args_delta: r#"{"command":"pwd"}"#.into(),
                },
                GatewayEvent::Usage(piko_protocol::Usage::empty()),
                GatewayEvent::Done("tool_use".into()),
            ])))
        } else {
            Ok(Box::pin(iter(vec![
                GatewayEvent::ContentDelta("done".into()),
                GatewayEvent::Usage(piko_protocol::Usage::empty()),
                GatewayEvent::Done("stop".into()),
            ])))
        }
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        Ok("done".into())
    }

    fn capabilities(&self) -> piko_protocol::model::ModelCapabilities {
        piko_protocol::model::ModelCapabilities::default()
    }
}

async fn create_open_session(
    repo_path: &std::path::Path,
    runner: Arc<OrchAgentRunRunner>,
) -> (HostServer, String, String) {
    let initial = HostServer::with_storage(JsonlSessionRepository::new(repo_path));
    let created = initial
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created");
    let listed = initial
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let session_path = listed
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionListed { sessions, .. }),
                ..
            } => sessions
                .iter()
                .find(|session| session.session_id == session_id)
                .and_then(|session| session.session_path.clone()),
            _ => None,
        })
        .expect("session path");
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    let root_agent_instance_id = root.agent_instance_id.clone();

    let server =
        HostServer::with_storage_and_runner(JsonlSessionRepository::new(repo_path), runner);
    server
        .handle_command(Command::SessionOpen {
            command_id: "open".into(),
            session_id: session_id.clone(),
            session_path: Some(session_path),
        })
        .await;
    server
        .handle_command(Command::AgentSubscribe {
            command_id: "subscribe".into(),
            session_id: session_id.clone(),
            agent_instance_id: root_agent_instance_id.clone(),
            after_seq: None,
        })
        .await;
    (server, session_id, root_agent_instance_id)
}

#[tokio::test]
async fn unanswered_approval_expires_fail_closed_and_ignores_late_response() {
    let temp = tempfile::tempdir().unwrap();
    let runner = Arc::new(
        OrchAgentRunRunner::new_with_mcp(
            Arc::new(ScriptedBashGateway::new()),
            "test",
            "test-key",
            "test-model",
            None,
            None,
            128_000,
            4_096,
            &[],
            None,
            Some(&ApprovalSettings {
                timeout_secs: Some(1),
            }),
            None,
            piko_hostd::telemetry::handle(),
        )
        .await,
    );
    let (server, session_id, root_agent_instance_id) =
        create_open_session(temp.path(), runner).await;

    let started = Instant::now();
    let events = tokio::time::timeout(
        Duration::from_secs(15),
        server.handle_command(Command::ChatSubmit {
            command_id: "submit".into(),
            session_id: session_id.clone(),
            target_agent_instance_id: root_agent_instance_id.clone(),
            text: "run it".into(),
        }),
    )
    .await
    .expect("turn completes after approval expiry");
    let elapsed = started.elapsed();

    let approval_id = events
        .iter()
        .find_map(|event| match event {
            ServerMessage::Approval(ApprovalEvent::Requested {
                approval_id,
                tool_name,
                ..
            }) if tool_name == "bash" => Some(approval_id.clone()),
            _ => None,
        })
        .expect("approval requested for bash");

    assert!(
        events.iter().any(|event| matches!(
            event,
            ServerMessage::Approval(ApprovalEvent::Resolved {
                decision: ApprovalDecision::Decline,
                ..
            })
        )),
        "expired approval resolves to a decline decision"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            ServerMessage::TurnLifecycle(piko_protocol::TurnEvent::Completed { .. })
        )),
        "turn is not stuck in WaitingForApproval"
    );
    assert!(
        events.iter().any(|event| {
            matches!(
                event,
                ServerMessage::TranscriptCommitted(committed)
                    if matches!(
                        &committed.message,
                        piko_protocol::Message::ToolResult { details: Some(details), .. }
                            if details.get("code").and_then(|v| v.as_str()) == Some("approval_expired")
                    )
            )
        }),
        "expired approval fails the tool call with approval_expired"
    );
    assert!(
        elapsed >= Duration::from_millis(800),
        "expiry must wait for the deadline, not decline instantly (elapsed {elapsed:?})"
    );

    // Late response after expiry is ignored: no second resolution event.
    let late = server
        .handle_command(Command::ApprovalRespond {
            command_id: "late".into(),
            session_id: session_id.clone(),
            approval_id,
            decision: ApprovalDecision::Accept,
            note: None,
        })
        .await;
    assert!(late.iter().any(|event| matches!(
        event,
        ServerMessage::CommandResponse {
            result: Ok(CommandResult::Empty),
            ..
        }
    )));
    assert!(
        !late.iter().any(|event| matches!(
            event,
            ServerMessage::Approval(ApprovalEvent::Resolved { .. })
        )),
        "late response must not publish a second resolution event"
    );

    // Snapshot no longer lists the expired approval.
    let snapshot = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let reconciled = snapshot
        .iter()
        .find_map(|event| match event {
            ServerMessage::SessionReconciled(reconciled) => Some(reconciled),
            _ => None,
        })
        .expect("reconciled snapshot");
    assert!(
        reconciled.snapshot.pending_approvals.is_empty(),
        "expired approval is removed from the pending set"
    );
}
