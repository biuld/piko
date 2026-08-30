use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use piko_llmd::gateway::{InferenceError, InferenceExecution, InferenceRequest};
use piko_protocol::execution::{CommitAck, CommitError, StartExecutionRequest};

use super::*;
use crate::domain::transcript::TranscriptSnapshot;

#[path = "tests_prepare.rs"]
mod prepare;

#[test]
fn context_budget_rejects_fixed_prompt_overhead_before_dispatch() {
    let prompt = piko_protocol::SemanticRunPrompt {
        blocks: vec![piko_protocol::PromptBlock {
            id: "large".into(),
            kind: piko_protocol::PromptBlockKind::Instruction,
            authority: piko_protocol::InstructionAuthority::Platform,
            trust: piko_protocol::ContentTrust::Trusted,
            source: piko_protocol::PromptSource::new("test", "large"),
            content: "x".repeat(4_000),
            content_digest: "digest".into(),
            cache_scope: piko_protocol::CacheScope::GlobalStable,
        }],
        ..Default::default()
    };
    let transcript = TranscriptSnapshot::new(vec![], vec![]);
    let error = super::budget::enforce_context_budget(
        &prompt,
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        100,
        50,
        false,
    )
    .expect_err("fixed overhead must fail closed");
    assert!(matches!(error, AgentApiError::ContextBudgetExceeded(_)));
}

#[test]
fn context_budget_accepts_request_below_window() {
    let transcript = TranscriptSnapshot::new(vec![], vec![]);
    let result = super::budget::enforce_context_budget(
        &piko_protocol::SemanticRunPrompt::default(),
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        10_000,
        100,
        false,
    );
    assert!(result.is_ok());
}

#[test]
fn context_budget_accounts_snapshot_and_reports_context_remaining() {
    let messages = vec![
        piko_protocol::Message::User {
            content: piko_protocol::messages::MessageContent::String("y".repeat(18_000)),
            timestamp: None,
        },
        piko_protocol::Message::ToolResult {
            tool_call_id: "call-1".into(),
            tool_name: Some("bash".into()),
            content: vec![piko_protocol::messages::ContentBlock::Text {
                text: "z".repeat(6_000),
            }],
            details: None,
            is_error: Some(false),
            timestamp: None,
        },
    ];
    let tokens = crate::domain::transcript::tokens::estimate_messages(&messages);
    let transcript = TranscriptSnapshot::new(messages, tokens);

    let estimate = super::budget::enforce_context_budget(
        &piko_protocol::SemanticRunPrompt::default(),
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        20_000,
        100,
        false,
    )
    .expect("below window");
    assert_eq!(estimate.transcript_tokens, transcript.total_tokens());
    assert_eq!(estimate.context_remaining, 20_000 - estimate.total);

    let error = super::budget::enforce_context_budget(
        &piko_protocol::SemanticRunPrompt::default(),
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        5_000,
        100,
        false,
    )
    .expect_err("over budget must fail closed");
    let message = error.to_string();
    assert!(message.contains("context_remaining=0"), "{message}");
    assert!(message.contains("compaction required"), "{message}");
}

#[test]
fn context_budget_caps_output_reserve_for_large_max_tokens_with_reasoning() {
    // F-35 / ADR-020: a reasoning model with max_tokens=384K in a 1M window
    // must not reserve 768K of fixed overhead. The reserve is capped at
    // OUTPUT_RESERVE_CAP and reasoning shares it.
    let prompt = piko_protocol::SemanticRunPrompt::default();
    let transcript = TranscriptSnapshot::new(vec![], vec![]);
    let estimate = super::budget::enforce_context_budget(
        &prompt,
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        1_000_000,
        384_000,
        true,
    )
    .expect("1M window with capped reserve must be accepted");
    // output=32_768 + reasoning=0 + margin=20_000 plus small prompt/tools
    // serialization overhead.
    assert!((32_768 + 20_000..=53_000).contains(&estimate.fixed_tokens));
    assert!(estimate.context_remaining > 900_000);
}

#[test]
fn context_budget_accepts_long_transcript_shape_from_production_turn() {
    // The failing production turn (session 75455f6c) estimated ~208K
    // transcript tokens in a 1M window and died because fixed overhead was
    // ~796K. With the capped reserve it must be accepted.
    let messages = vec![piko_protocol::Message::User {
        content: piko_protocol::messages::MessageContent::String("x".repeat(600_000)),
        timestamp: None,
    }];
    let tokens = crate::domain::transcript::tokens::estimate_messages(&messages);
    let transcript = TranscriptSnapshot::new(messages, tokens);
    let estimate = super::budget::enforce_context_budget(
        &piko_protocol::SemanticRunPrompt::default(),
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        1_000_000,
        384_000,
        true,
    )
    .expect("~200K transcript in a 1M window must be accepted");
    assert!(estimate.transcript_tokens >= 200_000);
    assert!(estimate.context_remaining > 700_000);
}

#[test]
fn context_budget_failure_message_reports_budget_fields_and_reasoning_flag() {
    let transcript = TranscriptSnapshot::new(vec![], vec![]);
    let error = super::budget::enforce_context_budget(
        &piko_protocol::SemanticRunPrompt::default(),
        &transcript,
        &[] as &[piko_llmd::tools::InferenceTool],
        1_000,
        384_000,
        true,
    )
    .expect_err("fixed overhead must fail closed");
    let message = error.to_string();
    for field in [
        "fixed estimate",
        "prompt=",
        "tools=",
        "output=",
        "reasoning=",
        "reasoning_enabled=true",
        "margin=",
        "window=1000",
    ] {
        assert!(message.contains(field), "missing {field:?} in {message:?}");
    }
}

struct NoopGateway;

#[async_trait]
impl InferenceGateway for NoopGateway {
    async fn start(
        &self,
        _req: InferenceRequest,
        _cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError> {
        Ok(InferenceExecution {
            events: Box::pin(tokio_stream::empty()),
            handle: None,
        })
    }
}

struct NoopCommit;

#[async_trait]
impl piko_orchd_api::ExecutionCommitPort for NoopCommit {
    async fn commit_message(
        &self,
        commit: piko_protocol::execution::MessageCommit,
    ) -> Result<CommitAck, CommitError> {
        Ok(CommitAck {
            session_id: commit.session_id,
            root_input_id: commit.root_input_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.message_id),
            revision: 1,
        })
    }

    async fn commit_model_step(
        &self,
        commit: piko_protocol::execution::ModelStepCommit,
    ) -> Result<CommitAck, CommitError> {
        Ok(CommitAck {
            session_id: commit.session_id,
            root_input_id: commit.root_input_id,
            agent_instance_id: commit.agent_instance_id,
            message_id: Some(commit.assistant.message_id),
            revision: 1,
        })
    }
}

fn request() -> StartExecutionRequest {
    StartExecutionRequest {
        request_id: "request".into(),
        session_id: "session".into(),
        source_turn_id: None,
        root_input_id: "execution".into(),
        agent_instance_id: "agent".into(),
        agent_spec: AgentSpec {
            id: "main".into(),
            version: "1".into(),
            provenance: piko_protocol::PromptSource::new("test", "main"),
            name: "main".into(),
            role: "test".into(),
            kind: piko_protocol::AgentKind::Supervisor,
            description: None,
            base_instructions: String::new(),
            model: None,
            thinking_level: None,
            tool_set_ids: Vec::new(),
            active_tool_names: None,
        },
        run_prompt: piko_protocol::SemanticRunPrompt {
            assembly_version: piko_protocol::AGENT_RUN_PROMPT_ASSEMBLY_VERSION,
            source_digest: "digest".into(),
            ..Default::default()
        },
        tool_catalog: piko_protocol::ResolvedToolCatalog::default(),
        world_state: None,
        inter_agent_completions: Vec::new(),
        user_mentions: Vec::new(),
        input_message_id: "message".into(),
        input: piko_protocol::MessageContent::String("hello".into()),
        context: piko_protocol::ConversationContext::empty(),
        config: piko_protocol::ExecutionConfig {
            agent_id: "main".into(),
            ..Default::default()
        },
    }
}

fn request_with(execution_id: &str, message_id: &str) -> StartExecutionRequest {
    StartExecutionRequest {
        root_input_id: execution_id.into(),
        input_message_id: message_id.into(),
        ..request()
    }
}

#[tokio::test]
async fn dropping_prepared_execution_releases_its_reservation() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();
    let prepared = runtime
        .prepare_execution(request(), HashMap::new(), tracing::Span::none())
        .await
        .unwrap();
    let scope = runtime.scope("session").await.unwrap();
    assert!(scope.get_execution("execution").await.is_some());
    drop(prepared);
    for _ in 0..100 {
        if scope.get_execution("execution").await.is_none() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("dropping PreparedExecution leaked its reservation");
}

#[tokio::test]
async fn world_state_is_committed_before_input_and_pushed_into_transcript() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();

    let mut plain = request();
    plain.context.head_message_id = Some("head-1".into());
    let prepared = runtime
        .prepare_execution(plain, HashMap::new(), tracing::Span::none())
        .await
        .unwrap();
    assert_eq!(
        prepared.input_commit.parent_message_id.as_deref(),
        Some("head-1"),
        "without world-state the input anchors on the pre-run head"
    );
    drop(prepared);

    let mut with_world_state = request_with("execution-2", "message-2");
    with_world_state.agent_instance_id = "agent-2".into();
    with_world_state.context.head_message_id = Some("head-1".into());
    with_world_state.world_state = Some(piko_protocol::Message::Context {
        content: piko_protocol::messages::MessageContent::String(
            "world-state changed since the previous run:\noperation_id: turn_2".into(),
        ),
        trust: piko_protocol::ContentTrust::Trusted,
        source: piko_protocol::PromptSource::new("run-state", "hostd/session"),
        timestamp: None,
    });
    let prepared = runtime
        .prepare_execution(with_world_state, HashMap::new(), tracing::Span::none())
        .await
        .unwrap();

    let world_state_id = piko_protocol::world_state_message_id("execution-2");
    let world = prepared
        .world_state_commit
        .as_ref()
        .expect("world-state commit");
    assert_eq!(world.message_id, world_state_id);
    assert_eq!(world.parent_message_id.as_deref(), Some("head-1"));
    assert_eq!(
        prepared.input_commit.parent_message_id.as_deref(),
        Some(world_state_id.as_str()),
        "the durable chain stays linear: head → world-state → input"
    );

    let actor = prepared.actor.as_ref().expect("actor");
    let messages = actor.transcript_messages();
    assert_eq!(messages.len(), 2, "world-state then input");
    assert!(matches!(
        messages[0],
        piko_protocol::Message::Context { .. }
    ));
    assert!(matches!(messages[1], piko_protocol::Message::User { .. }));
}

#[tokio::test]
async fn inter_agent_completions_chain_after_world_state_before_input() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();

    let mut request = request_with("execution-3", "message-3");
    request.context.head_message_id = Some("head-1".into());
    request.world_state = Some(piko_protocol::Message::Context {
        content: piko_protocol::messages::MessageContent::String("world-state".into()),
        trust: piko_protocol::ContentTrust::Trusted,
        source: piko_protocol::PromptSource::new("run-state", "hostd/session"),
        timestamp: None,
    });
    let report = piko_protocol::AgentWorkReport {
        agent_instance_id: "child".into(),
        root_input_id: "input-child".into(),
        report_id: "report-7".into(),
        outcome: piko_protocol::ExecutionOutcome::Succeeded {
            usage: piko_protocol::Usage::default(),
        },
        summary: "child done".into(),
        usage: piko_protocol::Usage::default(),
        artifacts: Vec::new(),
    };
    request.inter_agent_completions =
        vec![piko_protocol::agent_completion_context_message(&report)];
    request.user_mentions = vec![piko_protocol::file_mention_context_message(
        "src/a.rs",
        piko_protocol::FileMentionBody::Ok("fn a() {}".into()),
    )];

    let prepared = runtime
        .prepare_execution(request, HashMap::new(), tracing::Span::none())
        .await
        .unwrap();
    let world_state_id = piko_protocol::world_state_message_id("execution-3");
    let completion_id = piko_protocol::agent_completion_message_id("report-7");
    let mention_id = piko_protocol::file_mention_message_id("execution-3", 0);
    assert_eq!(prepared.completion_commits.len(), 1);
    assert_eq!(prepared.completion_commits[0].message_id, completion_id);
    assert_eq!(
        prepared.completion_commits[0].parent_message_id.as_deref(),
        Some(world_state_id.as_str())
    );
    assert_eq!(prepared.mention_commits.len(), 1);
    assert_eq!(prepared.mention_commits[0].message_id, mention_id);
    assert_eq!(
        prepared.mention_commits[0].parent_message_id.as_deref(),
        Some(completion_id.as_str())
    );
    assert_eq!(
        prepared.input_commit.parent_message_id.as_deref(),
        Some(mention_id.as_str()),
        "chain is head → world-state → completion → mention → input"
    );

    let messages = prepared
        .actor
        .as_ref()
        .expect("actor")
        .transcript_messages();
    assert_eq!(messages.len(), 4, "world-state, completion, mention, user");
    assert!(matches!(
        messages[1],
        piko_protocol::Message::Context { .. }
    ));
    assert!(matches!(
        messages[2],
        piko_protocol::Message::Context { .. }
    ));
    assert!(matches!(messages[3], piko_protocol::Message::User { .. }));
}

#[tokio::test]
async fn aborting_task_that_owns_prepared_execution_releases_reservation() {
    let runtime = AgentExecutionRuntime::new(Arc::new(NoopGateway));
    runtime
        .attach_session(
            "session".into(),
            SessionExecutionPorts::new(Arc::new(NoopCommit)),
        )
        .await
        .unwrap();
    let prepared = runtime
        .prepare_execution(request(), HashMap::new(), tracing::Span::none())
        .await
        .unwrap();
    let scope = runtime.scope("session").await.unwrap();
    let owner = tokio::spawn(async move {
        let _prepared = prepared;
        std::future::pending::<()>().await;
    });
    owner.abort();
    let _ = owner.await;
    for _ in 0..100 {
        if scope.get_execution("execution").await.is_none() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("aborting PreparedExecution owner leaked its reservation");
}
