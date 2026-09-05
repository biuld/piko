use std::sync::Arc;

use async_trait::async_trait;
use piko_llmd::gateway::{
    InferenceError, InferenceEvent, InferenceExecution, InferenceGateway, InferenceRequest,
};
use tokio_stream::{StreamExt, iter};
use tokio_util::sync::CancellationToken;

use piko_orchd_api::AgentCommitPort;
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::infra::storage::SessionStore;
use crate::ports::AgentRunRunner;

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};
use super::run::{ensure_root_tool_sets, resolve_recovered_agent_spec};

use piko_orchd_api::{ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest};
use piko_protocol::agents::HostSessionContext;

use crate::domain::config::{GuardianSettings, McpServerConfig, McpSettings, SafetySettings};
use crate::domain::guardian::{GuardianDecision, GuardianReviewCallback};

struct FailingAgentCommitPort;

struct DirectInputGateway;

async fn guardian_runner(
    model_executor: Arc<dyn InferenceGateway>,
    review: GuardianReviewCallback,
    max_consecutive_denials: u32,
) -> super::OrchAgentRunRunner {
    let runner = super::OrchAgentRunRunner::new_with_mcp(
        model_executor,
        "test",
        "model",
        None,
        128_000,
        4_096,
        &[],
        None,
        None,
        None,
        Some(&GuardianSettings {
            enabled: Some(true),
            model: None,
            provider: None,
            timeout_secs: Some(1),
            max_consecutive_denials: Some(max_consecutive_denials),
        }),
        None,
        None,
        None,
        None,
        crate::telemetry::handle(),
    )
    .await;
    runner.set_guardian_review_callback(review);
    // The user-approval fallback persists a PendingActionRequested fact before
    // registering the pending approval; give the session a durable route.
    ensure_test_active_work(&runner, "s1").await;
    runner
}

async fn safety_runner(safety: Option<&SafetySettings>) -> super::OrchAgentRunRunner {
    super::OrchAgentRunRunner::new_with_mcp(
        Arc::new(DirectInputGateway),
        "test",
        "model",
        None,
        128_000,
        4_096,
        &[],
        None,
        None,
        None,
        None,
        safety,
        None,
        None,
        None,
        crate::telemetry::handle(),
    )
    .await
}

async fn mcp_template_runner(
    templates: std::collections::HashMap<String, String>,
) -> super::OrchAgentRunRunner {
    let configs = vec![McpServerConfig {
        name: "github".into(),
        command: "echo".into(),
        args: vec![],
        env: std::collections::HashMap::new(),
        timeout_ms: None,
    }];
    super::OrchAgentRunRunner::new_with_mcp(
        Arc::new(DirectInputGateway),
        "test",
        "model",
        None,
        128_000,
        4_096,
        &configs,
        Some(&McpSettings {
            connect_timeout_ms: None,
            approval_templates: templates,
        }),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        crate::telemetry::handle(),
    )
    .await
}

async fn ensure_test_active_work(runner: &super::OrchAgentRunRunner, session_id: &str) {
    if runner
        .commit_routers
        .lock()
        .unwrap()
        .contains_key(session_id)
    {
        return;
    }
    let session_dir = tempfile::tempdir().unwrap().keep();
    let store = SessionStore::create_session(
        &session_dir,
        session_id.into(),
        "/tmp/piko-prompt-test".into(),
        1,
    )
    .unwrap();
    let generated_root = store.ensure_root_agent("main").unwrap();
    let spec = AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "Main".into(),
        role: "assistant".into(),
        kind: piko_protocol::AgentKind::Supervisor,
        description: None,
        base_instructions: "test".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: Vec::new(),
        active_tool_names: None,
    };
    store
        .commit_agent_command(
            session_id,
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: session_id.into(),
                    agent_instance_id: "root".into(),
                    agent_spec_id: spec.id.clone(),
                    parent_agent_instance_id: Some(generated_root.agent_instance_id),
                },
                spec: spec.clone(),
                origin_root_input_id: None,
                origin_tool_call_id: None,
            },
        )
        .await
        .unwrap();
    runner
        .prepare_session_runtime(
            session_id,
            "/tmp/piko-prompt-test",
            &session_dir,
            &spec,
            None,
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            session_id,
            AgentDurableCommand::AgentInputProcessingStarted {
                agent_instance_id: "root".into(),
                root_input_id: "input-root".into(),
                request_id: "request-root".into(),
                detached_recipient_agent_instance_id: None,
                prompt_assembly_version: 1,
                prompt_digest: "prompt-root".into(),
                started_at: 2,
                input: piko_protocol::AgentInput {
                    input_id: "input-root".into(),
                    request_id: "request-root".into(),
                    session_id: session_id.into(),
                    agent_instance_id: "root".into(),
                    origin: piko_protocol::AgentInputOrigin::User,
                    delivery: piko_protocol::AgentInputDelivery::FollowUp,
                    content: piko_protocol::MessageContent::String("test input".into()),
                    submitted_at: 2,
                    caller_agent_instance_id: None,
                    detached_recipient_agent_instance_id: None,
                },
                input_message_id: "message-root".into(),
                input_parent_message_id: None,
                input_tree_parent_entry_id: None,
                input_committed_at: 2,
            },
        )
        .await
        .unwrap();
}

fn approval_request(session_id: &str, tool_name: &str, id: &str) -> ToolApprovalRequest {
    let tool_args = if tool_name == "exec_command" {
        serde_json::json!({
            "cmd": "cargo test",
            "sandbox_permissions": "require_escalated",
            "justification": "test elevated approval flow"
        })
    } else {
        serde_json::json!({ "cmd": "cargo test" })
    };
    ToolApprovalRequest {
        tool_entity_id: id.into(),
        call_id: format!("call-{id}"),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        root_input_id: "input-root".into(),
        agent_role: None,
        provider_id: None,
        tool_name: tool_name.into(),
        tool_args,
        host_context: Some(HostSessionContext::new(session_id)),
        writable_roots: None,
    }
}

#[async_trait]
impl InferenceGateway for DirectInputGateway {
    async fn start(
        &self,
        _: InferenceRequest,
        _: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        Ok(InferenceExecution {
            events: Box::pin(iter(vec![
                InferenceEvent::text("child reply"),
                InferenceEvent::Usage(piko_protocol::Usage::empty()),
                InferenceEvent::completed("stop"),
            ])),
            handle: None,
        })
    }
}

#[async_trait]
impl AgentCommitPort for FailingAgentCommitPort {
    async fn commit_agent_command(
        &self,
        _: &str,
        _: AgentDurableCommand,
    ) -> Result<AgentCommitAck, CommitError> {
        Err(CommitError::Unavailable)
    }
}

fn create_command() -> AgentDurableCommand {
    AgentDurableCommand::Create {
        identity: AgentInstanceIdentity {
            session_id: "session".into(),
            agent_instance_id: "child".into(),
            agent_spec_id: "worker".into(),
            parent_agent_instance_id: Some("root".into()),
        },
        spec: AgentSpec {
            id: "worker".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "worker"),
            name: "Worker".into(),
            role: "worker".into(),
            kind: piko_protocol::AgentKind::Supervisor,
            description: None,
            base_instructions: "work".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
        origin_root_input_id: None,
        origin_tool_call_id: None,
    }
}

fn write_request(
    session_id: &str,
    tool_name: &str,
    id: &str,
    path: &str,
    writable_roots: Option<Vec<String>>,
) -> ToolApprovalRequest {
    ToolApprovalRequest {
        tool_entity_id: id.into(),
        call_id: format!("call-{id}"),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        root_input_id: "input-root".into(),
        agent_role: None,
        provider_id: None,
        tool_name: tool_name.into(),
        tool_args: serde_json::json!({ "path": path, "content": "x" }),
        host_context: Some(HostSessionContext::new(session_id)),
        writable_roots,
    }
}

async fn user_flow_resolves(
    runner: &super::OrchAgentRunRunner,
    request: ToolApprovalRequest,
    expected_pending_id: &str,
) -> ToolApprovalDecision {
    let session_id = request
        .host_context
        .as_ref()
        .expect("approval test request has a session")
        .session_id
        .clone();
    ensure_test_active_work(runner, &session_id).await;
    let runner_for_spawn = runner.clone();
    let pending =
        tokio::spawn(async move { runner_for_spawn.request_tool_approval(request).await });
    for _ in 0..200 {
        if runner
            .pending_approvals
            .lock()
            .unwrap()
            .contains_key(expected_pending_id)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        runner
            .pending_approvals
            .lock()
            .unwrap()
            .contains_key(expected_pending_id),
        "request must reach the user flow"
    );
    let responded = runner
        .respond_approval(expected_pending_id, piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed")
}

mod guardian;
mod mcp;
mod permissions;
mod safety;
mod tool_sets;
mod work;
