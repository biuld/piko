use crate::api::SessionTreeEntry;
use crate::application::host_app::HostApp;
use crate::application::sessions::helpers::session_reconciled_message;
use crate::domain::compaction::{
    CompactTrigger, CompactionSettings, DEFAULT_MIN_GROWTH_FRACTION, DEFAULT_MIN_GROWTH_TOKENS,
    active_branch_entries, compact_trigger, context_entries_after_compaction,
    estimate_context_tokens, min_growth_default,
};
use crate::util::{ClientEventSender, send_event};
use piko_protocol::command::CompactMode;

/// Fixed checkpoint message for a token-budget compact (F-05): history is
/// dropped without a model summarization call.
pub const NEW_CONTEXT_WINDOW_MESSAGE: &str =
    "A new context window was started without summarizing conversation history.";

/// Resolve the hysteresis guard (F-05 slice 2): an explicitly configured
/// `min_growth_tokens` wins; otherwise derive it from the resolved context
/// window via the fraction (defaulting to `DEFAULT_MIN_GROWTH_FRACTION`).
pub(crate) fn effective_min_growth_tokens(
    configured: Option<u64>,
    fraction: Option<f64>,
    context_window: u64,
) -> u64 {
    configured.unwrap_or_else(|| {
        min_growth_default(
            context_window,
            Some(fraction.unwrap_or(DEFAULT_MIN_GROWTH_FRACTION)),
        )
    })
}

impl HostApp {
    pub(crate) async fn resolved_model_context_window(&self) -> u64 {
        let (model, provider, fallback) = {
            let settings = self.settings.lock().await;
            let reserve = settings
                .compaction
                .as_ref()
                .and_then(|value| value.reserve_tokens)
                .unwrap_or(16384);
            let recent = settings
                .compaction
                .as_ref()
                .and_then(|value| value.keep_recent_tokens)
                .unwrap_or(20000);
            (
                settings.default_model.clone(),
                settings.default_provider.clone(),
                reserve + recent,
            )
        };
        self.model_registry
            .lock()
            .await
            .resolve(model.as_deref(), provider.as_deref())
            .map(|resolved| resolved.model.context_window)
            .filter(|window| *window > 0)
            .unwrap_or(fallback)
    }

    /// Compact the root shard of a session when the budget-window trigger
    /// fires (auto) or on request (`force`, e.g. `session.compact` or the
    /// `new_context_window` tool).
    ///
    /// `mode == Summarize` summarizes the dropped prefix with the model;
    /// `mode == NewContextWindow` drops history without a model call and
    /// keeps the most recent user message.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn compact_session_if_needed(
        &self,
        session_id: &str,
        agent_instance_id: &str,
        context_window: u64,
        mode: CompactMode,
        force: bool,
        tx: Option<&ClientEventSender>,
    ) -> Result<(), String> {
        let (c_settings, summarizer_model, summarizer_provider) = {
            let settings = self.settings.lock().await;
            let compaction = settings.compaction.as_ref();
            (
                CompactionSettings {
                    enabled: compaction.and_then(|c| c.enabled).unwrap_or(true),
                    reserve_tokens: compaction.and_then(|c| c.reserve_tokens).unwrap_or(16384),
                    keep_recent_tokens: compaction
                        .and_then(|c| c.keep_recent_tokens)
                        .unwrap_or(20000),
                    min_growth_tokens: compaction
                        .map(|c| {
                            effective_min_growth_tokens(
                                c.min_growth_tokens,
                                c.min_growth_fraction,
                                context_window,
                            )
                        })
                        .unwrap_or(DEFAULT_MIN_GROWTH_TOKENS),
                },
                compaction.and_then(|c| c.summarizer_model.clone()),
                compaction.and_then(|c| c.summarizer_provider.clone()),
            )
        };
        if !force && !c_settings.enabled {
            return Ok(());
        }

        let state_lock = self.state.lock().await;
        let Ok(session) = state_lock.session(session_id) else {
            return Ok(());
        };
        let root_agent_instance_id = format!("agent_{session_id}_root");
        if agent_instance_id != root_agent_instance_id {
            // SessionTreeEntry compaction currently projects the root shard.
            // Never compact a different AgentInstance through root state.
            return Ok(());
        }
        if session.compaction.pending {
            // A concurrent compaction owns the rewrite; skip without error.
            return Ok(());
        }
        let mut branch_entries =
            active_branch_entries(&session.entries, session.current_leaf_id.as_deref());
        // Compaction tips with no parent collapse the branch to a single stub;
        // fall back to the full tree so context_entries_after_compaction can expand.
        if branch_entries.len() <= 1 {
            branch_entries = session.entries.clone();
        }
        drop(state_lock);

        let context_entries = context_entries_after_compaction(&branch_entries);
        let estimate = estimate_context_tokens(&context_entries);

        if !force {
            let state = {
                let state_lock = self.state.lock().await;
                let session = state_lock
                    .session(session_id)
                    .map_err(|error| error.to_string())?;
                session.compaction.clone()
            };
            if !matches!(
                compact_trigger(&estimate, context_window, &c_settings, &state),
                CompactTrigger::Trigger
            ) {
                return Ok(());
            }
        }

        // Claim the pending slot atomically; re-evaluate under the lock so two
        // racing turns cannot both rewrite.
        let window_number_before;
        {
            let mut state = self.state.lock().await;
            let session = state
                .session_mut(session_id)
                .map_err(|error| error.to_string())?;
            if session.compaction.pending {
                return Ok(());
            }
            if !force {
                let trigger =
                    compact_trigger(&estimate, context_window, &c_settings, &session.compaction);
                if !matches!(trigger, CompactTrigger::Trigger) {
                    return Ok(());
                }
            }
            session.compaction.pending = true;
            window_number_before = session.compaction.window_number;
        }

        let result = self
            .run_compact_rewrite(
                session_id,
                &context_entries,
                mode,
                force,
                &c_settings,
                summarizer_model.as_deref(),
                summarizer_provider.as_deref(),
                window_number_before,
                tx,
            )
            .await;

        if result.is_err() {
            let mut state = self.state.lock().await;
            if let Ok(session) = state.session_mut(session_id) {
                session.compaction.pending = false;
            }
        }

        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_compact_rewrite(
        &self,
        session_id: &str,
        context_entries: &[SessionTreeEntry],
        mode: CompactMode,
        force: bool,
        settings: &CompactionSettings,
        summarizer_model: Option<&str>,
        summarizer_provider: Option<&str>,
        window_number_before: u64,
        tx: Option<&ClientEventSender>,
    ) -> Result<(), String> {
        let (_cut_index, entries_to_summarize, retained_entries, trigger) = match mode {
            CompactMode::Summarize => {
                let cut_point = crate::domain::compaction::find_cut_point(
                    context_entries,
                    0,
                    context_entries.len(),
                    settings.keep_recent_tokens,
                );
                let mut cut_index = cut_point.first_kept_entry_index;
                // Manual compact forces a rewrite even when the keep_recent
                // waterline would retain the entire short branch.
                if force && cut_index == 0 && context_entries.len() > 1 {
                    cut_index = context_entries.len() - 1;
                }
                if cut_index == 0 {
                    return Ok(());
                }
                (
                    cut_index,
                    &context_entries[0..cut_index],
                    &context_entries[cut_index..],
                    "manual",
                )
            }
            CompactMode::NewContextWindow => {
                let cut_index = context_entries
                    .iter()
                    .rposition(|entry| {
                        matches!(
                            entry,
                            SessionTreeEntry::Message(message_entry)
                                if matches!(message_entry.message, crate::api::Message::User { .. })
                        )
                    })
                    .ok_or_else(|| {
                        "new context window requires a user message to retain".to_string()
                    })?;
                if cut_index == 0 {
                    return Err(
                        "new context window requires history before the latest user message"
                            .to_string(),
                    );
                }
                (
                    cut_index,
                    &context_entries[0..cut_index],
                    &context_entries[cut_index..],
                    "new_context_window",
                )
            }
        };

        let summary = if mode == CompactMode::Summarize {
            let previous_summary =
                entries_to_summarize
                    .iter()
                    .rev()
                    .find_map(|entry| match entry {
                        SessionTreeEntry::Compaction(compaction) => {
                            Some(compaction.summary.as_str())
                        }
                        _ => None,
                    });

            let executor_guard = self.model_executor.lock().await;
            let Some(executor) = executor_guard.as_ref().cloned() else {
                return Ok(());
            };
            let (default_model_id, default_provider) = {
                let settings = self.settings.lock().await;
                (
                    settings
                        .default_model
                        .clone()
                        .unwrap_or_else(|| "default".into()),
                    settings
                        .default_provider
                        .clone()
                        .unwrap_or_else(|| "default".into()),
                )
            };

            let make_model = |model_id: String, provider: String| piko_protocol::messages::Model {
                id: model_id.clone(),
                name: model_id,
                provider,
                base_url: None,
            };
            let summarizer = summarizer_model.map(|model_id| {
                make_model(
                    model_id.to_string(),
                    summarizer_provider.unwrap_or(&default_provider).to_string(),
                )
            });
            let default_model = make_model(default_model_id.clone(), default_provider.clone());

            let fallback_from_override = summarizer.is_some();
            let first_model = summarizer.unwrap_or(default_model.clone());
            match crate::domain::compaction::summarizer::summarize_history(
                executor.clone(),
                first_model.clone(),
                entries_to_summarize,
                previous_summary,
                "",
            )
            .await
            {
                Ok(text) => Some(text),
                Err(error) if fallback_from_override => {
                    tracing::warn!(
                        session_id,
                        model = %first_model.id,
                        error = %error,
                        "compaction summarizer failed; falling back to the default model"
                    );
                    crate::domain::compaction::summarizer::summarize_history(
                        executor,
                        default_model,
                        entries_to_summarize,
                        previous_summary,
                        "",
                    )
                    .await
                    .ok()
                }
                Err(error) => {
                    tracing::warn!(
                        session_id,
                        model = %first_model.id,
                        error = %error,
                        "compaction summarization failed with the default model"
                    );
                    None
                }
            }
        } else {
            Some(NEW_CONTEXT_WINDOW_MESSAGE.to_string())
        };
        let Some(summary) = summary else {
            return Ok(());
        };

        let tokens_before = estimate_context_tokens(context_entries).tokens;
        let tokens_after = estimate_context_tokens(retained_entries).tokens;
        let first_kept_id = retained_entries
            .first()
            .map(|entry| entry.id().to_string())
            .unwrap_or_default();
        // Attach under the previous tip so the active branch still reaches messages.
        let parent_id = context_entries.last().map(|entry| entry.id().to_string());
        let details = serde_json::json!({
            "trigger": trigger,
            "windowNumber": window_number_before + 1,
            "tokensBefore": tokens_before,
            "tokensAfter": tokens_after,
        });

        let mut state = self.state.lock().await;
        let mut compacted = false;
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(session_id).cloned()
            };
            if let Some(path) = path
                && let Ok(entry) = storage.append_compaction(
                    &path,
                    parent_id.as_deref(),
                    &summary,
                    &first_kept_id,
                    None,
                    tokens_before,
                    Some(details),
                )
            {
                let _ = state.append_entry(session_id, entry);
                compacted = true;
                // A rewritten transcript no longer guarantees the retained
                // world-state snapshot (F-04 slice 2): clear the durable
                // baseline so the next run re-injects full.
                let _ = storage.set_world_state_baseline(&path, None);
            }
        }
        if let Ok(session) = state.session_mut(session_id) {
            session.compaction.pending = false;
            session.compaction.window_number = window_number_before + 1;
            session.compaction.rearm_tokens = Some(tokens_after);
            session.world_state_baseline = None;
        }
        drop(state);

        // H2: compact that rewrites the projected tree must rebuild via reconcile.
        if compacted {
            let reconcile = self
                .session_view(session_id)
                .await
                .ok()
                .map(|(snapshot, agents)| {
                    session_reconciled_message(
                        session_id.to_string(),
                        piko_protocol::ReconcileReason::ExplicitRefresh,
                        snapshot,
                        agents,
                    )
                });
            if let (Some(tx), Some(reconcile)) = (tx, reconcile) {
                send_event(tx, reconcile).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_core::Stream;
    use piko_llmd::auth::AuthStorage;
    use piko_llmd::gateway::{GatewayEvent, GatewayRequest, LlmGateway};
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
        async fn chat_stream(
            &self,
            _req: GatewayRequest,
            _cancel: Option<CancellationToken>,
        ) -> Result<Pin<Box<dyn Stream<Item = GatewayEvent> + Send + 'static>>, String> {
            Err("not used".into())
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
                api: "test".into(),
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
adapter = "openai"
base_url = "https://example.test/v1"

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
}
