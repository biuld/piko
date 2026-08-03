use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_core::Stream;
use piko_llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
use tokio_stream::{StreamExt, iter};
use tokio_util::sync::CancellationToken;

use piko_orchd_api::AgentCommitPort;
use piko_protocol::AgentInstanceIdentity;
use piko_protocol::agents::AgentSpec;
use piko_protocol::{AgentCommitAck, AgentDurableCommand, CommitError};

use crate::infra::storage::SessionStore;
use crate::ports::{AgentOperationAddress, AgentRunInput, AgentRunRunner};

use super::agent_commit::{EphemeralAgentCommitPort, ProjectingAgentCommitPort};
use super::run::{ensure_root_tool_sets, resolve_recovered_agent_spec};

use piko_orchd_api::{ApprovalGateway, ToolApprovalDecision, ToolApprovalRequest};
use piko_protocol::agents::HostSessionContext;

use crate::domain::config::{GuardianSettings, McpServerConfig, McpSettings, SafetySettings};
use crate::domain::guardian::{GuardianDecision, GuardianReviewCallback};

struct FailingAgentCommitPort;

struct DirectInputGateway;

async fn guardian_runner(
    model_executor: Arc<dyn LlmGateway>,
    review: GuardianReviewCallback,
    max_consecutive_denials: u32,
) -> super::OrchAgentRunRunner {
    let runner = super::OrchAgentRunRunner::new_with_mcp(
        model_executor,
        "test",
        "key",
        "model",
        None,
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
    runner
}

async fn safety_runner(safety: Option<&SafetySettings>) -> super::OrchAgentRunRunner {
    super::OrchAgentRunRunner::new_with_mcp(
        Arc::new(DirectInputGateway),
        "test",
        "key",
        "model",
        None,
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
        "key",
        "model",
        None,
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

fn approval_request(session_id: &str, tool_name: &str, id: &str) -> ToolApprovalRequest {
    ToolApprovalRequest {
        tool_entity_id: id.into(),
        call_id: format!("call-{id}"),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: None,
        tool_name: tool_name.into(),
        tool_args: serde_json::json!({ "cmd": "cargo test" }),
        host_context: Some(HostSessionContext::new(session_id)),
        writable_roots: None,
    }
}

#[async_trait]
impl LlmGateway for DirectInputGateway {
    async fn chat_stream(
        &self,
        _: GatewayRequest,
        _: Option<CancellationToken>,
    ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
        Ok(Box::pin(iter(vec![
            GatewayEvent::ContentDelta("child reply".into()),
            GatewayEvent::Usage(piko_protocol::Usage::empty()),
            GatewayEvent::Done("stop".into()),
        ])))
    }

    async fn llm_call(
        &self,
        _: piko_protocol::Model,
        _: Option<String>,
        _: Vec<piko_protocol::Message>,
        _: piko_protocol::ModelRunSettings,
    ) -> Result<String, String> {
        Ok("child reply".into())
    }

    fn capabilities(&self) -> piko_protocol::ModelCapabilities {
        piko_protocol::ModelCapabilities::default()
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
            description: None,
            base_instructions: "work".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
    }
}

#[tokio::test]
async fn agent_projection_is_emitted_only_after_durable_ack() {
    let hub = Arc::new(piko_orchd::events::SessionOutputHub::new(
        "session-1".into(),
        "epoch".into(),
        4,
    ));
    let event_router = Arc::new(super::observation_router::SessionObservationRouter::default());
    event_router.register("session-1", "operation", "child", true, Arc::clone(&hub));
    let cursor = hub.cursor();
    let subscription = hub.subscribe(&cursor).await.unwrap();
    let mut output = piko_orchd::events::merged_output_stream(subscription, cursor);
    let committing = ProjectingAgentCommitPort::new(
        Arc::new(EphemeralAgentCommitPort::default()),
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
    );
    committing
        .commit_agent_command("session", create_command())
        .await
        .unwrap();
    let envelope = output.next().await.unwrap().unwrap();
    assert!(matches!(
        envelope.output,
        piko_protocol::agent_runtime::SessionOutput::Event(event)
            if matches!(&event.event,
                piko_protocol::agent_runtime::SessionEvent::AgentChanged { agent }
                    if agent.agent_instance_id == "child")
    ));
    let cursor_after_success = hub.cursor();

    let failing = ProjectingAgentCommitPort::new(
        Arc::new(FailingAgentCommitPort),
        "session-1".into(),
        &[],
        Arc::clone(&event_router),
    );
    assert!(
        failing
            .commit_agent_command("session", create_command())
            .await
            .is_err()
    );
    assert_eq!(hub.cursor(), cursor_after_success);
}

#[tokio::test]
async fn direct_input_runs_the_addressed_recovered_child_agent() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        SessionStore::create_session(temp.path(), "session-direct".into(), "/project".into(), 1)
            .unwrap();
    let root = store.ensure_root_agent("main").unwrap();
    let child_id = "agent-child";
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: child_id.into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    version: "1".into(),
                    provenance: piko_protocol::PromptSource::new("test", "worker"),
                    name: "Worker".into(),
                    role: "worker".into(),
                    description: None,
                    base_instructions: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();
    store
        .commit_agent_command(
            "session-direct",
            AgentDurableCommand::Create {
                identity: AgentInstanceIdentity {
                    session_id: "session-direct".into(),
                    agent_instance_id: "agent-child-two".into(),
                    agent_spec_id: "worker".into(),
                    parent_agent_instance_id: Some(root.agent_instance_id.clone()),
                },
                spec: AgentSpec {
                    id: "worker".into(),
                    version: "1".into(),
                    provenance: piko_protocol::PromptSource::new("test", "worker"),
                    name: "Worker".into(),
                    role: "worker".into(),
                    description: None,
                    base_instructions: "Respond directly".into(),
                    model: None,
                    thinking_level: None,
                    tool_set_ids: Vec::new(),
                    active_tool_names: None,
                },
            },
        )
        .await
        .unwrap();

    let runner = super::OrchAgentRunRunner::new(
        Arc::new(DirectInputGateway),
        "test",
        "test-key",
        "test-model",
    )
    .await;
    let run = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-direct".into(),
            agent_instance_id: child_id.into(),
            prompt: "follow up".into(),
            source_turn_id: Some("run-direct".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            resume_agent: None,
        })
        .await
        .unwrap();
    AgentRunRunner::finish_agent_run(
        &runner,
        &AgentOperationAddress {
            session_id: "session-direct".into(),
            operation_id: "stale-run-id".into(),
            agent_instance_id: child_id.into(),
        },
        &piko_protocol::agent_runtime::SessionCursor {
            epoch: "stale".into(),
            seq: 0,
        },
    )
    .await;
    let duplicate = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-duplicate".into(),
            agent_instance_id: child_id.into(),
            prompt: "duplicate".into(),
            source_turn_id: Some("run-duplicate".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            resume_agent: None,
        })
        .await;
    assert_eq!(
        duplicate.unwrap().receipt.disposition,
        piko_protocol::InputDisposition::Queued
    );
    let second = runner
        .run_agent(AgentRunInput {
            session_id: "session-direct".into(),
            operation_id: "run-second-child".into(),
            agent_instance_id: "agent-child-two".into(),
            prompt: "parallel".into(),
            source_turn_id: Some("run-second-child".into()),
            prompt_resources: None,
            cwd: "/project".into(),
            active_tool_names: Some(Vec::new()),
            session_dir: temp.path().to_path_buf(),
            resume_agent: None,
        })
        .await
        .expect("different AgentInstances may run concurrently");
    let completed = run.process.wait_completion().await.unwrap();
    let second_completed = second.process.wait_completion().await.unwrap();
    assert_eq!(completed.address.agent_instance_id, child_id);
    assert!(completed.result.is_ok());
    assert!(second_completed.result.is_ok());

    let recovered = store.load_agent("session-direct", child_id).unwrap();
    assert_eq!(recovered.transcript.len(), 4);
    assert!(matches!(
        &recovered.transcript[0].message,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String(text),
            ..
        } if text == "follow up"
    ));
    assert!(matches!(
        &recovered.transcript[2].message,
        piko_protocol::Message::User {
            content: piko_protocol::MessageContent::String(text),
            ..
        } if text == "duplicate"
    ));
}

#[test]
fn ensure_root_tool_sets_adds_user_interaction_and_multi_agent() {
    let mut spec = AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        base_instructions: "hi".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec!["todo".into(), "workspace".into()],
        active_tool_names: None,
    };
    ensure_root_tool_sets(&mut spec);
    assert_eq!(
        spec.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "user_interaction".to_string(),
            "multi_agent".to_string()
        ]
    );
}

#[test]
fn resolve_recovered_agent_spec_prefers_durable_snapshot_then_registry_fallback() {
    let root_agent_spec = AgentSpec {
        id: "main".into(),
        version: "1".into(),
        provenance: piko_protocol::PromptSource::new("test", "main"),
        name: "Main".into(),
        role: "root".into(),
        description: None,
        base_instructions: "stable root prompt".into(),
        model: None,
        thinking_level: None,
        tool_set_ids: vec![
            "todo".into(),
            "workspace".into(),
            "user_interaction".into(),
            "multi_agent".into(),
        ],
        active_tool_names: None,
    };
    let mut resolved_specs = std::collections::HashMap::new();
    resolved_specs.insert(
        "main".into(),
        AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "Main".into(),
            role: "root".into(),
            description: None,
            base_instructions: "raw toml".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into()],
            active_tool_names: None,
        },
    );
    resolved_specs.insert(
        "coder".into(),
        AgentSpec {
            id: "coder".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "coder"),
            name: "Coder".into(),
            role: "worker".into(),
            description: None,
            base_instructions: "code".into(),
            model: None,
            thinking_level: None,
            tool_set_ids: vec!["todo".into(), "workspace".into(), "multi_agent".into()],
            active_tool_names: None,
        },
    );

    let root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        None,
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(root.base_instructions, "stable root prompt");
    assert!(root.tool_set_ids.iter().any(|id| id == "multi_agent"));
    assert!(root.tool_set_ids.iter().any(|id| id == "user_interaction"));

    let durable_root = resolve_recovered_agent_spec(
        "agent_session_root",
        "agent_session_root",
        Some(resolved_specs["main"].clone()),
        "main",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(durable_root.base_instructions, "raw toml");
    assert!(
        !durable_root
            .tool_set_ids
            .iter()
            .any(|id| id == "multi_agent")
    );

    let child = resolve_recovered_agent_spec(
        "agent_coder_1",
        "agent_session_root",
        None,
        "coder",
        &resolved_specs,
        &root_agent_spec,
    );
    assert_eq!(child.base_instructions, "code");
    assert_eq!(
        child.tool_set_ids,
        vec![
            "todo".to_string(),
            "workspace".to_string(),
            "multi_agent".to_string()
        ]
    );
    assert!(!child.tool_set_ids.iter().any(|id| id == "user_interaction"));
}

#[tokio::test]
async fn guardian_allow_executes_one_shot_without_store_grant() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            Ok(GuardianDecision {
                allow: true,
                reason: "build check".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let first = runner
        .request_tool_approval(approval_request("s1", "bash", "a1"))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);

    // One-shot semantics: an identical second call is reviewed again rather
    // than served from a session/workspace/permanent grant.
    let second = runner
        .request_tool_approval(approval_request("s1", "bash", "a2"))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn guardian_deny_fails_closed_and_breaker_escalates_to_user_then_resets() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            Ok(GuardianDecision {
                allow: false,
                reason: "outside workspace".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 2).await;

    let first = runner
        .request_tool_approval(approval_request("s1", "bash", "a1"))
        .await;
    assert!(matches!(
        first,
        ToolApprovalDecision::GuardianDenied { reason } if reason == "outside workspace"
    ));

    let second = runner
        .request_tool_approval(approval_request("s1", "bash", "a2"))
        .await;
    assert!(matches!(
        second,
        ToolApprovalDecision::GuardianDenied { .. }
    ));

    // Third request: breaker tripped, so the user flow owns the decision.
    let runner_for_spawn = runner.clone();
    let third = tokio::spawn(async move {
        runner_for_spawn
            .request_tool_approval(approval_request("s1", "bash", "a3"))
            .await
    });
    for _ in 0..200 {
        if runner.pending_approvals.lock().unwrap().contains_key("a3") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        runner.pending_approvals.lock().unwrap().contains_key("a3"),
        "tripped guardian must escalate to the user flow"
    );
    let responded = runner
        .respond_approval("a3", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), third)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // A user decision reset the breaker: the loop reviews again.
    let fourth = runner
        .request_tool_approval(approval_request("s1", "bash", "a4"))
        .await;
    assert!(matches!(
        fourth,
        ToolApprovalDecision::GuardianDenied { .. }
    ));
}

#[tokio::test]
async fn guardian_failure_fails_closed_without_running() {
    let review: GuardianReviewCallback =
        Arc::new(|_, _| Box::pin(async { Err::<GuardianDecision, _>("model down".into()) }));
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let decision = runner
        .request_tool_approval(approval_request("s1", "bash", "a1"))
        .await;
    assert_eq!(decision, ToolApprovalDecision::GuardianUnavailable);
}

#[tokio::test]
async fn guardian_timeout_fails_closed() {
    let review: GuardianReviewCallback = Arc::new(|_, _| {
        Box::pin(async {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(GuardianDecision {
                allow: true,
                reason: "late".into(),
            })
        })
    });
    let runner = guardian_runner(Arc::new(DirectInputGateway), review, 3).await;

    let decision = runner
        .request_tool_approval(approval_request("s1", "bash", "a1"))
        .await;
    assert_eq!(decision, ToolApprovalDecision::GuardianUnavailable);
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

#[tokio::test]
async fn safety_auto_approves_in_roots_write_one_shot_without_grant() {
    let runner = safety_runner(None).await;
    let roots = Some(vec!["/workspace".into()]);

    let first = runner
        .request_tool_approval(write_request(
            "s1",
            "edit",
            "a1",
            "/workspace/src/lib.rs",
            roots.clone(),
        ))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a constrained write"
    );

    // One-shot: an identical second call is assessed again (and accepted
    // again) rather than served from a store grant.
    let second = runner
        .request_tool_approval(write_request(
            "s1",
            "write",
            "a2",
            "/workspace/notes.md",
            roots,
        ))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_rejects_out_of_roots_write_with_reason() {
    let runner = safety_runner(None).await;
    let decision = runner
        .request_tool_approval(write_request(
            "s1",
            "write",
            "a1",
            "/Users/me/.ssh/authorized_keys",
            Some(vec!["/workspace".into()]),
        ))
        .await;
    match decision {
        ToolApprovalDecision::SafetyRejected { reason } => {
            assert!(reason.contains("/Users/me/.ssh/authorized_keys"));
        }
        other => panic!("expected SafetyRejected, got {other:?}"),
    }
    assert!(runner.pending_approvals.lock().unwrap().is_empty());
}

#[tokio::test]
async fn safety_without_writable_roots_falls_through_to_user_flow() {
    let runner = safety_runner(None).await;
    let decision = user_flow_resolves(
        &runner,
        write_request("s1", "edit", "a1", "src/lib.rs", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_opt_out_keeps_user_flow_for_in_roots_write() {
    let runner = safety_runner(Some(&SafetySettings {
        auto_approve_workspace_writes: Some(false),
    }))
    .await;
    let decision = user_flow_resolves(
        &runner,
        write_request(
            "s1",
            "edit",
            "a1",
            "/workspace/src/lib.rs",
            Some(vec!["/workspace".into()]),
        ),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn safety_never_assesses_non_write_tools() {
    let runner = safety_runner(None).await;
    let mut request = approval_request("s1", "bash", "a1");
    request.writable_roots = Some(vec!["/workspace".into()]);
    let decision = user_flow_resolves(&runner, request, "a1").await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn mcp_approval_template_prompt_reaches_the_user_snapshot() {
    let runner = mcp_template_runner(std::collections::HashMap::from([
        (
            "github/create_issue".into(),
            "This creates a GitHub issue in the configured repository.".into(),
        ),
        (
            "delete_resource".into(),
            "Delete {tool} on {server} with args {args}".into(),
        ),
    ]))
    .await;

    // server/tool template renders into the pending snapshot prompt.
    let request = ToolApprovalRequest {
        tool_entity_id: "mcp-1".into(),
        call_id: "call-mcp-1".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("github".into()),
        tool_name: "create_issue".into(),
        tool_args: serde_json::json!({ "title": "x" }),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending =
        tokio::spawn(async move { runner_for_spawn.request_tool_approval(request).await });
    let snapshot_prompt = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            // Read the snapshot in its own statement so the std Mutex guard
            // drops before the await below (single-thread runtime must not
            // hold a sync lock across .await).
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-1")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            if let Some(prompt) = prompt {
                break prompt;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("snapshot appears in time");
    assert_eq!(
        snapshot_prompt.as_deref(),
        Some("This creates a GitHub issue in the configured repository.")
    );
    let responded = runner
        .respond_approval("mcp-1", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // Bare `tool` fallback substitutes placeholders.
    let bare = ToolApprovalRequest {
        tool_entity_id: "mcp-2".into(),
        call_id: "call-mcp-2".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("github".into()),
        tool_name: "delete_resource".into(),
        tool_args: serde_json::json!({ "id": 7 }),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending = tokio::spawn(async move { runner_for_spawn.request_tool_approval(bare).await });
    let bare_prompt = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-2")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            if let Some(prompt) = prompt {
                break prompt;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("snapshot appears in time");
    let bare_prompt = bare_prompt.expect("bare tool template resolves");
    assert!(
        bare_prompt.contains("Delete delete_resource on github"),
        "{bare_prompt}"
    );
    assert!(bare_prompt.contains("{\"id\":7}"), "{bare_prompt}");
    let responded = runner
        .respond_approval("mcp-2", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);

    // A non-MCP provider is never matched by a bare `tool` key.
    let non_mcp = ToolApprovalRequest {
        tool_entity_id: "mcp-3".into(),
        call_id: "call-mcp-3".into(),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: None,
        provider_id: Some("workspace".into()),
        tool_name: "delete_resource".into(),
        tool_args: serde_json::json!({}),
        host_context: Some(HostSessionContext::new("s1")),
        writable_roots: None,
    };
    let runner_for_spawn = runner.clone();
    let pending =
        tokio::spawn(async move { runner_for_spawn.request_tool_approval(non_mcp).await });
    let prompt_absent = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let prompt = {
                let pending = runner.pending_approvals.lock().unwrap();
                pending
                    .get("mcp-3")
                    .map(|entry| entry.snapshot.prompt.clone())
            };
            match prompt {
                Some(prompt) => break prompt,
                None => {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            }
        }
    })
    .await
    .expect("snapshot appears in time");
    assert!(
        prompt_absent.is_none(),
        "bare tool keys must not match non-MCP tools"
    );
    let responded = runner
        .respond_approval("mcp-3", piko_protocol::ApprovalDecision::Accept)
        .await
        .expect("response accepted");
    assert!(responded);
    let decision = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("user decision resolves the request")
        .expect("spawned request task completed");
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn mcp_statuses_reports_configured_servers() {
    let runner = mcp_template_runner(std::collections::HashMap::new()).await;
    let statuses = runner.mcp_statuses().await;
    // The fixture `echo` server cannot speak JSON-RPC, so the entry exists
    // but reports disconnected with the connect error.
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name, "github");
    assert!(!statuses[0].connected);
    assert!(statuses[0].error.is_some());
}

async fn permission_runner(
    settings: Option<&crate::domain::config::PermissionsSettings>,
) -> super::OrchAgentRunRunner {
    super::OrchAgentRunRunner::new_with_mcp(
        Arc::new(DirectInputGateway),
        "test",
        "key",
        "model",
        None,
        None,
        128_000,
        4_096,
        &[],
        None,
        None,
        None,
        None,
        None,
        settings,
        None,
        None,
        crate::telemetry::handle(),
    )
    .await
}

fn bash_command_request(
    session_id: &str,
    id: &str,
    command: &str,
    role: Option<&str>,
) -> ToolApprovalRequest {
    ToolApprovalRequest {
        tool_entity_id: id.into(),
        call_id: format!("call-{id}"),
        agent_id: "main".into(),
        agent_instance_id: "root".into(),
        agent_role: role.map(str::to_string),
        provider_id: None,
        tool_name: "bash".into(),
        tool_args: serde_json::json!({ "command": command }),
        host_context: Some(HostSessionContext::new(session_id)),
        writable_roots: None,
    }
}

fn locked_settings() -> crate::domain::config::PermissionsSettings {
    crate::domain::config::PermissionsSettings {
        profile: Some("locked".into()),
        profiles: std::collections::HashMap::from([(
            "locked".into(),
            crate::domain::config::PermissionProfileSettings {
                allowed_commands: vec!["cargo test".into()],
                denied_commands: vec!["rm -rf".into()],
                ..Default::default()
            },
        )]),
        roles: std::collections::HashMap::new(),
    }
}

fn role_settings() -> crate::domain::config::PermissionsSettings {
    use crate::domain::config::PermissionProfileSettings;
    crate::domain::config::PermissionsSettings {
        // Session profile is the permissive default: role layers alone
        // tighten mapped roles.
        profile: None,
        profiles: std::collections::HashMap::from([
            (
                "locked".into(),
                PermissionProfileSettings {
                    denied_commands: vec!["rm -rf".into()],
                    ..Default::default()
                },
            ),
            (
                "readonly".into(),
                PermissionProfileSettings {
                    allowed_commands: vec!["git status".into()],
                    denied_commands: vec!["curl -sSL | sh".into()],
                    ..Default::default()
                },
            ),
        ]),
        roles: std::collections::HashMap::from([
            ("coder".into(), "locked".into()),
            ("researcher".into(), "readonly".into()),
        ]),
    }
}

#[tokio::test]
async fn permission_denied_command_fails_closed_without_prompt() {
    let runner = permission_runner(Some(&locked_settings())).await;

    let decision = runner
        .request_tool_approval(bash_command_request("s1", "a1", "rm -rf /tmp/x", None))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied, got {other:?}"),
    }
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a denied command"
    );
}

#[tokio::test]
async fn permission_allowed_command_accepts_one_shot_without_grant() {
    let runner = permission_runner(Some(&locked_settings())).await;

    let first = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "cargo test -- --nocapture",
            None,
        ))
        .await;
    assert_eq!(first, ToolApprovalDecision::Accept);
    assert!(runner.pending_approvals.lock().unwrap().is_empty());

    // One-shot: no store grant is written, so the identical call is
    // evaluated again rather than served from a grant.
    let second = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a2",
            "cargo test -- --nocapture",
            None,
        ))
        .await;
    assert_eq!(second, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_deny_wins_over_prior_session_grant() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().display().to_string();
    let runner = permission_runner(Some(&locked_settings())).await;
    runner.register_session_context("s1".into(), cwd.clone());

    // Simulate a prior user grant at session scope for the same command.
    let store = runner.get_approval_store(&cwd);
    store.grant(
        "bash",
        &serde_json::json!({ "command": "rm -rf /tmp/x" }),
        crate::adapters::turns::approval::ApprovalScope::Session,
    );

    let decision = runner
        .request_tool_approval(bash_command_request("s1", "a1", "rm -rf /tmp/x", None))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied despite prior grant, got {other:?}"),
    }
}

#[tokio::test]
async fn permission_non_matching_command_keeps_user_flow() {
    let runner = permission_runner(Some(&locked_settings())).await;
    let decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a1", "ls -la", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_role_denied_command_fails_closed_for_mapped_role() {
    let runner = permission_runner(Some(&role_settings())).await;

    // The mapped "coder" role denies `rm -rf` without any prompt.
    let decision = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "rm -rf /tmp/x",
            Some("coder"),
        ))
        .await;
    match decision {
        ToolApprovalDecision::PermissionDenied { reason } => {
            assert!(reason.contains("rm -rf"));
        }
        other => panic!("expected PermissionDenied for mapped role, got {other:?}"),
    }
    assert!(
        runner.pending_approvals.lock().unwrap().is_empty(),
        "no user prompt for a role-denied command"
    );

    // An unmapped role keeps the session flow (session profile has no
    // command rules), so the same command reaches the user.
    let root_decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a2", "rm -rf /tmp/x", Some("root")),
        "a2",
    )
    .await;
    assert_eq!(root_decision, ToolApprovalDecision::Accept);

    // A missing role on the request also inherits the session profile.
    let none_decision = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a3", "rm -rf /tmp/x", None),
        "a3",
    )
    .await;
    assert_eq!(none_decision, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_role_allowed_command_accepts_one_shot_for_mapped_role() {
    let runner = permission_runner(Some(&role_settings())).await;

    let researcher = runner
        .request_tool_approval(bash_command_request(
            "s1",
            "a1",
            "git status",
            Some("researcher"),
        ))
        .await;
    assert_eq!(researcher, ToolApprovalDecision::Accept);
    assert!(runner.pending_approvals.lock().unwrap().is_empty());

    // A role mapped to a different profile is not affected by "readonly"'s
    // allow rules and keeps the session flow.
    let coder = user_flow_resolves(
        &runner,
        bash_command_request("s1", "a2", "git status", Some("coder")),
        "a2",
    )
    .await;
    assert_eq!(coder, ToolApprovalDecision::Accept);
}

#[tokio::test]
async fn permission_non_command_tools_are_unaffected() {
    let runner = permission_runner(Some(&locked_settings())).await;
    let decision = user_flow_resolves(
        &runner,
        write_request("s1", "edit", "a1", "src/lib.rs", None),
        "a1",
    )
    .await;
    assert_eq!(decision, ToolApprovalDecision::Accept);
}
