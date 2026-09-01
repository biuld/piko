//! PR-1: `AgentInterrupt` command vec includes Cancelling/RequiresAction
//! `SessionReconciled`. Idle interrupt does not push.

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::HostServer;
use piko_orchd_api::AgentCommitPort;
use piko_protocol::{
    AgentDurableCommand, AgentForeground, AgentWorkViewState, PendingActionSummary,
};
use support::work_snapshot::{create_session, processing_started, push_snapshots, work_for};

#[derive(Clone, Default)]
struct JournalInterruptRunner {
    session_dir: Arc<std::sync::Mutex<Option<PathBuf>>>,
    target: Arc<std::sync::Mutex<Option<(String, String, String)>>>,
    accepted: bool,
}

#[async_trait]
impl AgentRunRunner for JournalInterruptRunner {
    async fn interrupt_agent(&self, session_id: &str, agent_instance_id: &str) -> bool {
        let Some((stored_session, stored_agent, root_input_id)) =
            self.target.lock().unwrap().clone()
        else {
            return false;
        };
        if stored_session != session_id || stored_agent != agent_instance_id {
            return false;
        }
        let session_dir = self
            .session_dir
            .lock()
            .unwrap()
            .clone()
            .expect("session dir");
        SessionStore::new(session_dir)
            .commit_agent_command(
                session_id,
                AgentDurableCommand::InterruptRequested {
                    agent_instance_id: agent_instance_id.into(),
                    root_input_id,
                    requested_at: 2,
                },
            )
            .await
            .expect("interrupt fact");
        self.accepted
    }
}

async fn seed_running_root(
    server: &HostServer,
    runner: &JournalInterruptRunner,
    root_input_id: &str,
) -> (String, String) {
    let (session_id, session_path) = create_session(server).await;
    *runner.session_dir.lock().unwrap() = Some(session_path.clone());
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            &session_id,
            processing_started(&session_id, &root.agent_instance_id, root_input_id),
        )
        .await
        .unwrap();
    *runner.target.lock().unwrap() = Some((
        session_id.clone(),
        root.agent_instance_id.clone(),
        root_input_id.into(),
    ));
    (session_id, root.agent_instance_id)
}

#[tokio::test]
async fn interrupt_command_stream_includes_cancelling_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let runner = Arc::new(JournalInterruptRunner {
        accepted: true,
        ..Default::default()
    });
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        runner.clone(),
    );
    let (session_id, agent_instance_id) =
        seed_running_root(&server, runner.as_ref(), "input-interrupt").await;

    let events = server
        .handle_command(Command::AgentInterrupt {
            command_id: "interrupt".into(),
            session_id,
            agent_instance_id: agent_instance_id.clone(),
        })
        .await;

    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::CommandResponse {
            result: Ok(CommandResult::AgentInterrupted { accepted: true, .. }),
            ..
        }
    )));
    let snapshots = push_snapshots(&events, &agent_instance_id);
    assert_eq!(snapshots.len(), 1, "exactly one push, no StateSnapshot");
    let work = work_for(&snapshots[0].snapshot, &agent_instance_id);
    assert_eq!(work.foreground, AgentForeground::Cancelling);
    assert_eq!(
        work.active_work.as_ref().map(|active| active.state),
        Some(AgentWorkViewState::Cancelling)
    );
}

#[tokio::test]
async fn idle_interrupt_does_not_push_session_reconciled() {
    let temp = tempfile::tempdir().unwrap();
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(JournalInterruptRunner::default()),
    );
    let (session_id, _) = create_session(&server).await;
    let events = server
        .handle_command(Command::AgentInterrupt {
            command_id: "interrupt-idle".into(),
            session_id,
            agent_instance_id: "agent-idle".into(),
        })
        .await;
    assert!(events.iter().any(|event| matches!(
        event,
        ServerMessage::CommandResponse {
            result: Ok(CommandResult::AgentInterrupted {
                accepted: false,
                ..
            }),
            ..
        }
    )));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ServerMessage::SessionReconciled(_)))
    );
}

#[tokio::test]
async fn interrupt_with_open_pending_action_pushes_requires_action() {
    let temp = tempfile::tempdir().unwrap();
    let runner = Arc::new(JournalInterruptRunner {
        accepted: false,
        ..Default::default()
    });
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        runner.clone(),
    );
    let (session_id, agent_instance_id) =
        seed_running_root(&server, runner.as_ref(), "input-both").await;
    let session_dir = runner.session_dir.lock().unwrap().clone().unwrap();
    SessionStore::new(session_dir)
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::PendingActionRequested {
                agent_instance_id: agent_instance_id.clone(),
                root_input_id: "input-both".into(),
                action: PendingActionSummary {
                    action_id: "appr-1".into(),
                    kind: "approval".into(),
                    summary: Some("shell".into()),
                },
                requested_at: 2,
            },
        )
        .await
        .unwrap();

    let events = server
        .handle_command(Command::AgentInterrupt {
            command_id: "interrupt".into(),
            session_id,
            agent_instance_id: agent_instance_id.clone(),
        })
        .await;
    let snapshots = push_snapshots(&events, &agent_instance_id);
    assert_eq!(snapshots.len(), 1);
    let work = work_for(&snapshots[0].snapshot, &agent_instance_id);
    assert_eq!(work.foreground, AgentForeground::RequiresAction);
    assert_eq!(
        work.active_work.as_ref().map(|active| active.state),
        Some(AgentWorkViewState::Cancelling)
    );
    assert_eq!(
        work.pending_action
            .as_ref()
            .map(|action| action.action_id.as_str()),
        Some("appr-1")
    );
}
