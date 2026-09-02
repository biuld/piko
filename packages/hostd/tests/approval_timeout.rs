//! F-07 acceptance: an unanswered tool-approval request expires after the
//! configured deadline, resolves fail-closed, and late responses are ignored.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use piko_hostd::adapters::OrchAgentRunRunner;
use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::domain::config::ApprovalSettings;
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::protocol::HostServer;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use piko_protocol::{ApprovalDecision, ApprovalEvent};
use tokio_stream::iter;
use tokio_util::sync::CancellationToken;

fn execution(events: Vec<InferenceEvent>) -> InferenceExecution {
    InferenceExecution {
        events: Box::pin(iter(events)),
        handle: None,
    }
}

/// Step 0 emits an elevated `exec_command` call (requires approval); every later step is a
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
impl InferenceGateway for ScriptedBashGateway {
    async fn start(
        &self,
        _request: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        let step = self.step.fetch_add(1, Ordering::SeqCst);
        if step == 0 {
            Ok(execution(vec![
                InferenceEvent::function_call(
                    "call-exec",
                    "exec_command",
                    r#"{"cmd":"pwd","sandbox_permissions":"require_escalated","justification":"exercise approval timeout"}"#,
                ),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("tool_use"),
            ]))
        } else {
            Ok(execution(vec![
                InferenceEvent::text("done"),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("stop"),
            ]))
        }
    }
}

async fn create_open_session(
    repo_path: &std::path::Path,
    runner: Arc<OrchAgentRunRunner>,
) -> (HostServer, String, String) {
    // Seed a workspace `main` agent so the root spec never depends on the
    // developer's real $PIKO_HOME install.
    let workspace = repo_path.join("workspace");
    let agents_dir = workspace.join(".piko").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/agents/main.toml"),
        agents_dir.join("main.toml"),
    )
    .unwrap();
    let cwd = workspace.to_string_lossy().into_owned();
    let initial = HostServer::with_storage(JsonlSessionRepository::new(repo_path));
    let created = initial
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd,
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
            "test-model",
            None,
            128_000,
            4_096,
            &[],
            None,
            None,
            Some(&ApprovalSettings {
                timeout_secs: Some(1),
            }),
            None,
            None,
            None,
            None,
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
        server.handle_command(Command::submit_follow_up(
            "submit",
            session_id.clone(),
            root_agent_instance_id.clone(),
            piko_protocol::MessageContent::String("run it".into()),
        )),
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
            }) if tool_name == "exec_command" => Some(approval_id.clone()),
            _ => None,
        })
        .expect("approval requested for exec_command");

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
            ServerMessage::SessionReconciled(reconciled)
                if reconciled.snapshot.agent_work.iter().all(|work| work.active_work.is_none())
        )),
        "agent work is not stuck in WaitingForApproval"
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
