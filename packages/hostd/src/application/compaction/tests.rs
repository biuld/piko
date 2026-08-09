#[cfg(test)]
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use piko_llmd::auth::AuthStorage;
use piko_llmd::gateway::{GatewayError, LlmGateway, ModelEventStream, ModelRequest};
use piko_llmd::providers::ProviderRegistry;
use piko_protocol::messages::{Message, Model};
use piko_protocol::model::{ModelCapabilities, ModelRunSettings};
use piko_protocol::{
    CommandResult, ContentBlock, MessageContent, MessageEntry, ServerMessage, SessionTreeEntry,
};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::domain::config::CompactionSettings as ConfigCompaction;
use crate::domain::config::HostSettings;
use crate::domain::config::ModelRegistry;
use crate::infra::storage::JsonlSessionRepository;

const WINDOW: u64 = 8_192;
/// 12.5% of the resolved 8k window: the derived hysteresis guard.
const DERIVED_GUARD: u64 = 1_024;

struct StubGateway;

#[async_trait]
impl LlmGateway for StubGateway {
    async fn execute(
        &self,
        _req: ModelRequest,
        _cancel: CancellationToken,
    ) -> Result<ModelEventStream, GatewayError> {
        Err(GatewayError::new(
            piko_llmd::gateway::ErrorClass::Upstream,
            "stub",
            "execute",
            "not used",
        ))
    }

    async fn llm_call(
        &self,
        _model: Model,
        _system_prompt: Option<String>,
        _messages: Vec<Message>,
        _settings: ModelRunSettings,
    ) -> Result<String, String> {
        Ok("## Goal\n- test compact".into())
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }
}

fn user_entry(id: &str, parent: Option<&str>, seq: u64, text: &str) -> SessionTreeEntry {
    SessionTreeEntry::Message(MessageEntry {
        id: id.into(),
        parent_id: parent.map(str::to_string),
        timestamp: seq.to_string(),
        agent_id: "main".into(),
        agent_instance_id: "agent-main".into(),
        source_turn_id: format!("turn-{id}"),
        transcript_seq: seq,
        message: Message::User {
            content: MessageContent::String(text.into()),
            timestamp: None,
        },
    })
}

fn assistant_entry(id: &str, parent: Option<&str>, seq: u64, text: &str) -> SessionTreeEntry {
    SessionTreeEntry::Message(MessageEntry {
        id: id.into(),
        parent_id: parent.map(str::to_string),
        timestamp: seq.to_string(),
        agent_id: "main".into(),
        agent_instance_id: "agent-main".into(),
        source_turn_id: format!("turn-{id}"),
        transcript_seq: seq,
        message: Message::Assistant {
            content: vec![ContentBlock::Text { text: text.into() }],
            continuation: None,
            provider: "test-provider".into(),
            model: "small-model".into(),
            usage: None,
            stop_reason: None,
            error_message: None,
            timestamp: None,
        },
    })
}

fn created_session_id(events: Vec<ServerMessage>) -> String {
    events
        .into_iter()
        .find_map(|event| match event {
            ServerMessage::CommandResponse {
                result: Ok(CommandResult::SessionCreated { session_id, .. }),
                ..
            } => Some(session_id),
            _ => None,
        })
        .expect("session created")
}

#[test]
fn explicit_min_growth_wins_over_fraction() {
    assert_eq!(
        effective_min_growth_tokens(Some(16_384), Some(0.125), WINDOW),
        16_384
    );
}

#[test]
fn unset_guard_derives_from_resolved_window() {
    assert_eq!(
        effective_min_growth_tokens(None, Some(0.125), WINDOW),
        DERIVED_GUARD
    );
    // The documented default fraction applies when unset.
    assert_eq!(
        effective_min_growth_tokens(None, None, WINDOW),
        DERIVED_GUARD
    );
    // A windowless resolution (force compact callback) keeps the constant.
    assert_eq!(
        effective_min_growth_tokens(None, Some(0.125), 0),
        DEFAULT_MIN_GROWTH_TOKENS
    );
}

#[tokio::test]
async fn window_fraction_guard_scales_retrigger_to_resolved_model() {
    let temp = tempfile::tempdir().unwrap();
    let app = HostApp::with_storage_runner_settings(
        JsonlSessionRepository::new(temp.path()),
        Arc::new(crate::ports::ErrorAgentRunRunner::new("not used")),
        HostSettings {
            default_model: Some("small-model".into()),
            default_provider: Some("test-provider".into()),
            compaction: Some(ConfigCompaction {
                enabled: Some(true),
                reserve_tokens: Some(1_024),
                keep_recent_tokens: Some(7_000),
                min_growth_tokens: None,
                min_growth_fraction: Some(0.125),
                summarizer_model: None,
                summarizer_provider: None,
            }),
            ..HostSettings::default()
        },
    );

    // Resolve "small-model" to an 8k context window via a test provider.
    let providers_dir = temp.path().join("providers");
    std::fs::create_dir_all(&providers_dir).unwrap();
    std::fs::write(
        providers_dir.join("test.toml"),
        r#"[provider]
id = "test-provider"

[api_surfaces.platform]
base_url = "https://example.test/v1"
auth_methods = ["api_key"]

[default_targets.platform]
protocol = "chat_completions"

[models.small-model]
name = "Small Model"
reasoning = false
input = ["text"]
context_window = 8192
max_tokens = 1024
"#,
    )
    .unwrap();
    let mut providers = ProviderRegistry::new();
    providers.load_from_dir(&providers_dir);
    *app.model_registry.lock().await =
        ModelRegistry::with_registry(AuthStorage::in_memory(HashMap::new()), vec![], providers);
    assert_eq!(app.resolved_model_context_window().await, WINDOW);

    let session_id = created_session_id(
        app.apply_session_create("create", "/project".into())
            .await
            .unwrap(),
    );
    let root_id = format!("agent_{session_id}_root");
    app.set_model_executor(Arc::new(StubGateway)).await;

    // Fill a branch past the 8k − 1k waterline (4 × 3k tokens).
    {
        let mut state = app.state.lock().await;
        let session = state.session_mut(&session_id).unwrap();
        session
            .entries
            .push(user_entry("u1", None, 1, &"x".repeat(12_000)));
        session
            .entries
            .push(assistant_entry("a1", Some("u1"), 2, &"x".repeat(12_000)));
        session
            .entries
            .push(user_entry("u2", Some("a1"), 3, &"x".repeat(12_000)));
        session
            .entries
            .push(assistant_entry("a2", Some("u2"), 4, &"x".repeat(12_000)));
        session.current_leaf_id = Some("a2".into());
    }

    // First window: triggers once past the waterline; keep_recent retains
    // 9k tokens, which becomes the rearm baseline.
    app.compact_session_if_needed(
        &session_id,
        &root_id,
        WINDOW,
        CompactMode::Summarize,
        false,
        None,
    )
    .await
    .unwrap();
    let (window_number, rearm, compaction_id) = {
        let state = app.state.lock().await;
        let session = state.session(&session_id).unwrap();
        assert_eq!(session.compaction.window_number, 1);
        let compaction_id = session.entries.last().unwrap().id().to_string();
        (
            session.compaction.window_number,
            session.compaction.rearm_tokens.unwrap(),
            compaction_id,
        )
    };
    assert_eq!(window_number, 1);
    assert_eq!(rearm, 9_000);

    // Growth of 400 tokens past the rearm baseline (< derived 1_024
    // guard): the fixed 16_384 default would hold forever on this 8k
    // window; the derived guard must also hold here.
    {
        let mut state = app.state.lock().await;
        let session = state.session_mut(&session_id).unwrap();
        session.entries.push(user_entry(
            "u3",
            Some(&compaction_id),
            5,
            &"y".repeat(1_200),
        ));
        session
            .entries
            .push(assistant_entry("a3", Some("u3"), 6, &"y".repeat(400)));
        session.current_leaf_id = Some("a3".into());
    }
    app.compact_session_if_needed(
        &session_id,
        &root_id,
        WINDOW,
        CompactMode::Summarize,
        false,
        None,
    )
    .await
    .unwrap();
    {
        let state = app.state.lock().await;
        assert_eq!(
            state.session(&session_id).unwrap().compaction.window_number,
            window_number
        );
    }

    // Growth of 1_200 tokens total (≥ derived 1_024 guard) re-triggers
    // the next window.
    {
        let mut state = app.state.lock().await;
        let session = state.session_mut(&session_id).unwrap();
        session
            .entries
            .push(user_entry("u4", Some("a3"), 7, &"z".repeat(2_800)));
        session
            .entries
            .push(assistant_entry("a4", Some("u4"), 8, &"z".repeat(400)));
        session.current_leaf_id = Some("a4".into());
    }
    app.compact_session_if_needed(
        &session_id,
        &root_id,
        WINDOW,
        CompactMode::Summarize,
        false,
        None,
    )
    .await
    .unwrap();
    {
        let state = app.state.lock().await;
        assert_eq!(
            state.session(&session_id).unwrap().compaction.window_number,
            window_number + 1
        );
    }
}
