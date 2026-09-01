use super::*;

use std::path::PathBuf;
use std::sync::Arc;

use piko_protocol::{AgentForeground, AgentWorkViewState, PendingActionSummary};

fn processing_started(
    session_id: &str,
    agent_instance_id: &str,
    root_input_id: &str,
) -> AgentDurableCommand {
    AgentDurableCommand::AgentInputProcessingStarted {
        agent_instance_id: agent_instance_id.into(),
        root_input_id: root_input_id.into(),
        request_id: root_input_id.into(),
        detached_recipient_agent_instance_id: None,
        prompt_assembly_version: 1,
        prompt_digest: "hydrate".into(),
        started_at: 1,
        input: piko_protocol::AgentInput {
            input_id: root_input_id.into(),
            request_id: root_input_id.into(),
            session_id: session_id.into(),
            agent_instance_id: agent_instance_id.into(),
            origin: piko_protocol::AgentInputOrigin::User,
            delivery: piko_protocol::AgentInputDelivery::FollowUp,
            content: MessageContent::String("run".into()),
            submitted_at: 1,
            caller_agent_instance_id: None,
            detached_recipient_agent_instance_id: None,
        },
        input_message_id: format!("msg-{root_input_id}"),
        input_parent_message_id: None,
        input_tree_parent_entry_id: None,
        input_committed_at: 1,
    }
}

fn work_for<'a>(
    snapshot: &'a piko_protocol::SessionSnapshot,
    agent_instance_id: &str,
) -> &'a piko_protocol::AgentWorkSnapshot {
    snapshot
        .agent_work
        .iter()
        .find(|work| work.agent_instance_id == agent_instance_id)
        .expect("agent work")
}

#[derive(Clone, Default)]
struct JournalCancelRunner {
    session_dir: Arc<std::sync::Mutex<Option<PathBuf>>>,
}

#[async_trait]
impl AgentRunRunner for JournalCancelRunner {
    async fn cancel_agent_input(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_protocol::AgentInputCancelReceipt, piko_hostd::api::ProtocolError> {
        let session_dir = self
            .session_dir
            .lock()
            .unwrap()
            .clone()
            .expect("session dir");
        SessionStore::new(session_dir)
            .commit_agent_command(
                session_id,
                AgentDurableCommand::AgentInputDispositionChanged {
                    change: piko_protocol::AgentInputDispositionChange {
                        agent_instance_id: agent_instance_id.into(),
                        input_id: input_id.into(),
                        disposition: piko_protocol::AgentInputDisposition::Cancelled,
                        root_input_id: None,
                        model_step_id: None,
                        changed_at: 20,
                    },
                },
            )
            .await
            .expect("cancel fact");
        Ok(piko_protocol::AgentInputCancelReceipt {
            input_id: input_id.into(),
            request_id: input_id.into(),
            session_id: session_id.into(),
            agent_instance_id: agent_instance_id.into(),
            accepted: true,
        })
    }
}

/// Hydrate (not push): `StateSnapshot` reads RequiresAction/Cancelling from
/// `current.json` without a later SessionOpen recovery.
#[tokio::test]
async fn state_snapshot_hydrates_requires_action_and_cancelling() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let server = HostServer::with_storage(repo);
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = session_id_from(&created);
    let listed = server
        .handle_command(Command::SessionList {
            command_id: "list".into(),
            scope: piko_protocol::SessionListScope::All,
            cwd: None,
        })
        .await;
    let session_path = listed
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(piko_hostd::api::CommandResult::SessionListed { sessions, .. }),
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
    store
        .commit_agent_command(
            &session_id,
            processing_started(&session_id, &root.agent_instance_id, "input-hydrate"),
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::PendingActionRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "input-hydrate".into(),
                action: PendingActionSummary {
                    action_id: "appr-hydrate".into(),
                    kind: "approval".into(),
                    summary: Some("shell".into()),
                },
                requested_at: 2,
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::InterruptRequested {
                agent_instance_id: root.agent_instance_id.clone(),
                root_input_id: "input-hydrate".into(),
                requested_at: 3,
            },
        )
        .await
        .unwrap();

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id,
        })
        .await;
    let snapshot = snapshot_from_refresh(&refresh);
    let work = work_for(snapshot, &root.agent_instance_id);
    assert_eq!(work.foreground, AgentForeground::RequiresAction);
    assert_eq!(
        work.active_work.as_ref().map(|active| active.state),
        Some(AgentWorkViewState::Cancelling)
    );
    assert_eq!(
        work.pending_action
            .as_ref()
            .map(|action| action.action_id.as_str()),
        Some("appr-hydrate")
    );
}

/// Two HostServers SessionOpen the same journal after unfinished work + queue
/// + pending steer. Recovery cancels the root and its steers; follow-ups stay
/// and can be cancelled by `input_id`. Steer against the successor is rejected.
#[tokio::test]
async fn two_clients_restart_cancel_queued_follow_up_and_reject_steer() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let created = repo.create("/tmp/project").unwrap();
    let session_id = created.state.session_id.clone();
    let session_path = created.path.clone();
    let store = SessionStore::new(&session_path);
    let root = store.ensure_root_agent("main").unwrap();
    store
        .commit_agent_command(
            &session_id,
            processing_started(&session_id, &root.agent_instance_id, "input-restart"),
        )
        .await
        .unwrap();
    store
        .commit_message(
            piko_protocol::agent_work::MessageCommit {
                session_id: session_id.clone(),
                root_input_id: "input-restart".into(),
                agent_instance_id: root.agent_instance_id.clone(),
                message_id: "msg-input-restart".into(),
                parent_message_id: None,
                tree_parent_entry_id: None,
                message: Message::User {
                    content: MessageContent::String("run".into()),
                    timestamp: Some(1),
                },
                committed_at: 1,
            },
            "main",
        )
        .unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput {
                        input_id: "steer-restart".into(),
                        request_id: "steer-restart".into(),
                        session_id: session_id.clone(),
                        agent_instance_id: root.agent_instance_id.clone(),
                        origin: piko_protocol::AgentInputOrigin::User,
                        delivery: piko_protocol::AgentInputDelivery::SteerActive,
                        content: MessageContent::String("late steer".into()),
                        submitted_at: 2,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: piko_protocol::AgentInputDisposition::PendingSteer,
                    root_input_id: Some("input-restart".into()),
                    admitted_at: 2,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            &session_id,
            AgentDurableCommand::AgentInputAdmitted {
                admission: piko_protocol::AgentInputAdmission {
                    input: piko_protocol::AgentInput {
                        input_id: "follow-restart".into(),
                        request_id: "follow-restart".into(),
                        session_id: session_id.clone(),
                        agent_instance_id: root.agent_instance_id.clone(),
                        origin: piko_protocol::AgentInputOrigin::User,
                        delivery: piko_protocol::AgentInputDelivery::FollowUp,
                        content: MessageContent::String("next".into()),
                        submitted_at: 3,
                        caller_agent_instance_id: None,
                        detached_recipient_agent_instance_id: None,
                    },
                    disposition: piko_protocol::AgentInputDisposition::PendingFollowUp,
                    root_input_id: None,
                    admitted_at: 3,
                },
            },
        )
        .await
        .unwrap();

    let runner = Arc::new(JournalCancelRunner {
        session_dir: Arc::new(std::sync::Mutex::new(Some(session_path.clone()))),
    });
    let first = HostServer::with_storage_and_runner(repo.clone(), runner.clone());
    let second = HostServer::with_storage_and_runner(repo, runner);
    let path = session_path.to_string_lossy().into_owned();
    let first_events = first
        .handle_command(Command::SessionOpen {
            command_id: "open-first".into(),
            session_id: session_id.clone(),
            session_path: Some(path.clone()),
        })
        .await;
    let second_events = second
        .handle_command(Command::SessionOpen {
            command_id: "open-second".into(),
            session_id: session_id.clone(),
            session_path: Some(path),
        })
        .await;
    let first_snapshot = snapshot_from_refresh(&first_events);
    let second_snapshot = snapshot_from_refresh(&second_events);
    assert_eq!(first_snapshot.agent_work, second_snapshot.agent_work);
    let work = work_for(first_snapshot, &root.agent_instance_id);
    assert!(work.active_work.is_none(), "unfinished root is recovered");
    assert!(
        work.pending_steers.is_empty(),
        "steers do not jump to a successor"
    );
    assert_eq!(work.queued_inputs[0].input_id, "follow-restart");

    let cancelled = first
        .handle_command(Command::AgentInputCancel {
            command_id: "cancel-follow".into(),
            session_id: session_id.clone(),
            agent_instance_id: root.agent_instance_id.clone(),
            input_id: "follow-restart".into(),
        })
        .await;
    assert!(cancelled.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Ok(piko_hostd::api::CommandResult::AgentInputCancelled { receipt, .. }),
            ..
        } if receipt.accepted && receipt.input_id == "follow-restart"
    )));
    let after_cancel = snapshot_from_refresh(&cancelled);
    assert!(
        work_for(after_cancel, &root.agent_instance_id)
            .queued_inputs
            .is_empty()
    );

    let steered = first
        .handle_command(Command::submit_steer(
            "steer-after-restart",
            session_id,
            root.agent_instance_id.clone(),
            MessageContent::String("should fail".into()),
        ))
        .await;
    assert!(steered.iter().any(|event| matches!(
        event,
        Event::CommandResponse {
            result: Err(message),
            ..
        } if message.contains("not running")
    )));
}
