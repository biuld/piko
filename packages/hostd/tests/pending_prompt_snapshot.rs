use std::sync::Arc;

use async_trait::async_trait;
use hostd::api::{
    ApprovalSnapshot, ApprovalStatus, Command, ServerMessage as Event, UserInteractionSnapshot,
    UserInteractionStatus,
};
use hostd::infra::storage::JsonlSessionRepository;
use hostd::ports::{TurnRunHandle, TurnRunInput, TurnRunner};
use hostd::protocol::HostServer;
use piko_protocol::{InteractionChoice, InteractionQuestion};

#[derive(Clone, Default)]
struct PendingPromptRunner {
    approvals: Arc<std::sync::Mutex<Vec<ApprovalSnapshot>>>,
    interactions: Arc<std::sync::Mutex<Vec<UserInteractionSnapshot>>>,
}

#[async_trait]
impl TurnRunner for PendingPromptRunner {
    async fn run_turn(
        &self,
        _input: TurnRunInput,
    ) -> Result<TurnRunHandle, hostd::api::ProtocolError> {
        Err(hostd::api::ProtocolError::InvalidCommand("not used".into()))
    }

    async fn pending_prompts_for_session(
        &self,
        _session_id: &str,
    ) -> (Vec<ApprovalSnapshot>, Vec<UserInteractionSnapshot>) {
        (
            self.approvals.lock().unwrap().clone(),
            self.interactions.lock().unwrap().clone(),
        )
    }
}

#[tokio::test]
async fn state_snapshot_includes_in_process_pending_prompts() {
    let temp = tempfile::tempdir().unwrap();
    let repo = JsonlSessionRepository::new(temp.path());
    let runner = PendingPromptRunner::default();
    runner.approvals.lock().unwrap().push(ApprovalSnapshot {
        approval_id: "appr-1".into(),
        tool_name: "bash".into(),
        request: serde_json::json!({ "cmd": "ls" }),
        status: ApprovalStatus::Pending,
    });
    runner
        .interactions
        .lock()
        .unwrap()
        .push(UserInteractionSnapshot {
            interaction_id: "ix-1".into(),
            agent_instance_id: "agent-1".into(),
            agent_id: "main".into(),
            tool_call_id: "call-1".into(),
            status: UserInteractionStatus::Pending,
            title: Some("Choose".into()),
            questions: vec![InteractionQuestion {
                id: "q1".into(),
                header: "Q".into(),
                prompt: "Pick one".into(),
                choices: vec![InteractionChoice {
                    id: "a".into(),
                    label: "A".into(),
                    value: serde_json::json!("a"),
                    input: None,
                }],
                required: true,
            }],
            require_confirm: false,
            auto_resolution_ms: None,
        });

    let server = HostServer::with_storage_and_runner(repo, Arc::new(runner));
    let created = server
        .handle_command(Command::SessionCreate {
            command_id: "create".into(),
            cwd: "/tmp/project".into(),
        })
        .await;
    let session_id = created
        .iter()
        .find_map(|event| match event {
            Event::CommandResponse {
                result: Ok(hostd::api::CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id.clone()),
            _ => None,
        })
        .expect("session created");

    let refresh = server
        .handle_command(Command::StateSnapshot {
            command_id: "snapshot".into(),
            session_id: session_id.clone(),
        })
        .await;
    let Event::SessionReconciled(reconciled) = refresh
        .iter()
        .find(|event| matches!(event, Event::SessionReconciled(_)))
        .expect("reconciled")
    else {
        unreachable!();
    };

    assert_eq!(reconciled.snapshot.pending_approvals.len(), 1);
    assert_eq!(reconciled.snapshot.pending_approvals[0].tool_name, "bash");
    assert_eq!(reconciled.snapshot.pending_interactions.len(), 1);
    assert_eq!(
        reconciled.snapshot.pending_interactions[0].interaction_id,
        "ix-1"
    );
}
