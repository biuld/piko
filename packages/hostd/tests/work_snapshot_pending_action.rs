//! PR-1: pending-action request/resolve push `SessionReconciled` on the
//! in-flight submit observation stream (not isolated `ApprovalRespond`).

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use piko_hostd::api::{Command, ServerMessage};
use piko_hostd::infra::storage::{JsonlSessionRepository, SessionStore};
use piko_hostd::ports::AgentRunRunner;
use piko_hostd::protocol::HostServer;
use piko_orchd_api::AgentCommitPort;
use piko_protocol::{
    AgentDurableCommand, AgentForeground, ApprovalDecision, ApprovalEvent, ApprovalSnapshot,
    ApprovalStatus, PendingActionSummary,
};
use support::work_snapshot::{create_session, processing_started, work_for};
use support::{MockRunHarness, MockSessionPublisher, success_report, test_oneshot};

#[derive(Clone, Default)]
struct PendingActionStreamRunner {
    harness: MockRunHarness,
    session_dir: Arc<std::sync::Mutex<Option<PathBuf>>>,
    publisher: Arc<std::sync::Mutex<Option<Arc<MockSessionPublisher>>>>,
    completion_tx: Arc<
        std::sync::Mutex<
            Option<tokio::sync::oneshot::Sender<piko_hostd::ports::AgentRunCompletion>>,
        >,
    >,
    root: Arc<std::sync::Mutex<Option<(String, String, String)>>>,
}

impl PendingActionStreamRunner {
    async fn commit(&self, session_id: &str, command: AgentDurableCommand) {
        let session_dir = self
            .session_dir
            .lock()
            .unwrap()
            .clone()
            .expect("session dir");
        SessionStore::new(session_dir)
            .commit_agent_command(session_id, command)
            .await
            .expect("commit");
    }
}

#[async_trait]
impl AgentRunRunner for PendingActionStreamRunner {
    async fn ensure_session_runtime(
        &self,
        _session_id: &str,
        _cwd: &str,
        session_dir: &std::path::Path,
        _resume_agent: Option<&piko_hostd::ports::ResumeAgent>,
    ) -> Result<(), piko_hostd::api::ProtocolError> {
        *self.session_dir.lock().unwrap() = Some(session_dir.to_path_buf());
        Ok(())
    }

    async fn submit_agent_input(
        &self,
        input: piko_protocol::AgentInput,
        _runtime: piko_orchd_api::AgentInputRuntime,
    ) -> Result<piko_protocol::AgentInputReceipt, piko_hostd::api::ProtocolError> {
        let session_id = input.session_id.clone();
        let input_id = input.input_id.clone();
        let agent_instance_id = input.agent_instance_id.clone();
        self.commit(
            &session_id,
            processing_started(&session_id, &agent_instance_id, &input_id),
        )
        .await;
        let (publisher, subscription) = MockSessionPublisher::new(session_id.clone());
        let (completion_tx, completion_rx) = test_oneshot();
        self.harness.register(
            &session_id,
            &input_id,
            subscription,
            completion_rx,
            publisher.clone(),
        );
        *self.publisher.lock().unwrap() = Some(publisher);
        *self.completion_tx.lock().unwrap() = Some(completion_tx);
        *self.root.lock().unwrap() = Some((
            session_id.clone(),
            agent_instance_id.clone(),
            input_id.clone(),
        ));
        let runner = self.clone();
        tokio::spawn(async move {
            let (session_id, agent_instance_id, input_id) =
                runner.root.lock().unwrap().clone().unwrap();
            runner
                .commit(
                    &session_id,
                    AgentDurableCommand::PendingActionRequested {
                        agent_instance_id: agent_instance_id.clone(),
                        root_input_id: input_id.clone(),
                        action: PendingActionSummary {
                            action_id: "appr-stream".into(),
                            kind: "approval".into(),
                            summary: Some("bash".into()),
                        },
                        requested_at: 2,
                    },
                )
                .await;
            runner.publisher.lock().unwrap().as_ref().unwrap().publish(
                agent_instance_id.clone(),
                "main",
                1,
                piko_protocol::agent_runtime::SessionEvent::ApprovalRequested {
                    approval: ApprovalSnapshot {
                        approval_id: "appr-stream".into(),
                        agent_instance_id,
                        root_input_id: input_id,
                        tool_name: "bash".into(),
                        request: serde_json::json!({"cmd":"ls"}),
                        prompt: None,
                        status: ApprovalStatus::Pending,
                    },
                },
            );
        });
        Ok(piko_protocol::AgentInputReceipt {
            input_id,
            request_id: input.request_id,
            session_id,
            agent_instance_id,
            disposition: piko_protocol::AgentInputDisposition::AppliedAsRoot,
            queued_position: None,
        })
    }

    async fn respond_approval(
        &self,
        approval_id: &str,
        _decision: ApprovalDecision,
    ) -> Result<bool, piko_hostd::api::ProtocolError> {
        let (session_id, agent_instance_id, root_input_id) =
            self.root.lock().unwrap().clone().expect("root");
        self.commit(
            &session_id,
            AgentDurableCommand::PendingActionResolved {
                agent_instance_id: agent_instance_id.clone(),
                root_input_id,
                action_id: approval_id.into(),
                resolved_at: 3,
            },
        )
        .await;
        self.publisher.lock().unwrap().as_ref().unwrap().publish(
            agent_instance_id,
            "main",
            2,
            piko_protocol::agent_runtime::SessionEvent::ApprovalResolved {
                approval_id: approval_id.into(),
                status: ApprovalStatus::Approved,
            },
        );
        Ok(true)
    }

    async fn wait_agent_input_started(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
        _disposition: piko_protocol::AgentInputDisposition,
    ) -> Result<piko_orchd_api::SessionSubscription, piko_hostd::api::ProtocolError> {
        Ok(self.harness.take_subscription(session_id, input_id))
    }

    async fn wait_agent_input_completion(
        &self,
        session_id: &str,
        _agent_instance_id: &str,
        input_id: &str,
    ) -> Result<piko_hostd::ports::AgentRunCompletion, piko_hostd::api::ProtocolError> {
        Ok(self.harness.completion(session_id, input_id).await)
    }

    async fn finish_agent_run(&self, session_id: &str, _agent_instance_id: &str, input_id: &str) {
        self.harness.finish(session_id, input_id);
    }
}

#[tokio::test]
async fn pending_action_request_and_resolve_push_on_submit_observation_stream() {
    let temp = tempfile::tempdir().unwrap();
    let runner = Arc::new(PendingActionStreamRunner::default());
    let server = HostServer::with_storage_and_runner(
        JsonlSessionRepository::new(temp.path()),
        runner.clone(),
    );
    let (session_id, session_path) = create_session(&server).await;
    let agent_instance_id = SessionStore::new(&session_path)
        .ensure_root_agent("main")
        .unwrap()
        .agent_instance_id;

    let mut stream = server.handle_command_stream(Command::submit_follow_up(
        "submit",
        session_id.clone(),
        agent_instance_id.clone(),
        piko_protocol::MessageContent::String("need approval".into()),
    ));
    let mut requested = Vec::new();
    while !requested.iter().any(|event| {
        matches!(
            event,
            ServerMessage::Approval(ApprovalEvent::Requested { approval_id, .. })
                if approval_id == "appr-stream"
        )
    }) || !requested.iter().any(|event| match event {
        ServerMessage::SessionReconciled(reconciled) => {
            let work = work_for(&reconciled.snapshot, &agent_instance_id);
            work.foreground == AgentForeground::RequiresAction
                && work
                    .pending_action
                    .as_ref()
                    .is_some_and(|action| action.action_id == "appr-stream")
        }
        _ => false,
    }) {
        requested.push(
            tokio::time::timeout(Duration::from_secs(2), stream.recv())
                .await
                .expect("request event")
                .expect("stream open"),
        );
    }
    let request_idx = requested
        .iter()
        .position(|event| {
            matches!(
                event,
                ServerMessage::Approval(ApprovalEvent::Requested { approval_id, .. })
                    if approval_id == "appr-stream"
            )
        })
        .unwrap();
    assert_eq!(
        requested[request_idx + 1..]
            .iter()
            .filter(|event| matches!(event, ServerMessage::SessionReconciled(_)))
            .count(),
        1,
        "exactly one RequiresAction snapshot after ApprovalRequested"
    );

    let _ = server
        .handle_command(Command::ApprovalRespond {
            command_id: "respond".into(),
            session_id: session_id.clone(),
            approval_id: "appr-stream".into(),
            decision: ApprovalDecision::Accept,
            note: None,
        })
        .await;
    let mut resolved = Vec::new();
    while !resolved.iter().any(|event| {
        matches!(
            event,
            ServerMessage::Approval(ApprovalEvent::Resolved { .. })
        )
    }) || !resolved.iter().any(|event| match event {
        ServerMessage::SessionReconciled(reconciled) => {
            work_for(&reconciled.snapshot, &agent_instance_id)
                .pending_action
                .is_none()
        }
        _ => false,
    }) {
        resolved.push(
            tokio::time::timeout(Duration::from_secs(2), stream.recv())
                .await
                .expect("resolve event")
                .expect("stream open"),
        );
    }
    let resolve_idx = resolved
        .iter()
        .position(|event| {
            matches!(
                event,
                ServerMessage::Approval(ApprovalEvent::Resolved { .. })
            )
        })
        .unwrap();
    assert_eq!(
        resolved[resolve_idx + 1..]
            .iter()
            .filter(|event| matches!(event, ServerMessage::SessionReconciled(_)))
            .count(),
        1,
        "exactly one cleared pending_action snapshot after resolve"
    );

    let (session_id, agent_instance_id, input_id) = runner.root.lock().unwrap().clone().unwrap();
    let completion_tx = runner.completion_tx.lock().unwrap().take().unwrap();
    let barrier = runner.harness.barrier_for(&session_id, &input_id).unwrap();
    let _ = completion_tx.send(piko_hostd::ports::AgentRunCompletion {
        input_id,
        result: Ok(success_report(&agent_instance_id)),
        observation_barrier: barrier,
    });
}
