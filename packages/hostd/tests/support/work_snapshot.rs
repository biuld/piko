use std::path::PathBuf;

use piko_hostd::api::{Command, CommandResult, ServerMessage};
use piko_hostd::protocol::HostServer;
use piko_protocol::AgentDurableCommand;

pub fn session_id_from(events: &[ServerMessage]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created")
}

pub fn session_path_from(events: &[ServerMessage], session_id: &str) -> PathBuf {
    events
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
        .expect("session path")
        .into()
}

pub async fn create_session(server: &HostServer) -> (String, PathBuf) {
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
    let session_path = session_path_from(&listed, &session_id);
    (session_id, session_path)
}

pub fn processing_started(
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
        prompt_digest: "push-test".into(),
        started_at: 1,
        input: piko_protocol::AgentInput {
            input_id: root_input_id.into(),
            request_id: root_input_id.into(),
            session_id: session_id.into(),
            agent_instance_id: agent_instance_id.into(),
            origin: piko_protocol::AgentInputOrigin::User,
            delivery: piko_protocol::AgentInputDelivery::FollowUp,
            content: piko_protocol::MessageContent::String("run".into()),
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

pub fn work_for<'a>(
    snapshot: &'a piko_protocol::SessionSnapshot,
    agent_instance_id: &str,
) -> &'a piko_protocol::AgentWorkSnapshot {
    snapshot
        .agent_work
        .iter()
        .find(|work| work.agent_instance_id == agent_instance_id)
        .expect("agent work")
}

pub fn push_snapshots<'a>(
    events: &'a [ServerMessage],
    agent_instance_id: &str,
) -> Vec<&'a piko_protocol::SessionReconciledEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            ServerMessage::SessionReconciled(reconciled)
                if reconciled
                    .snapshot
                    .agent_work
                    .iter()
                    .any(|work| work.agent_instance_id == agent_instance_id) =>
            {
                Some(reconciled)
            }
            _ => None,
        })
        .collect()
}
