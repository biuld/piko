use super::*;

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

    /// Compact the root AgentInstance transcript when the budget-window trigger
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
            // SessionTreeEntry compaction currently projects the root transcript.
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
        let estimate = ContextUsageEstimate::from_tokens(
            self.transcript_estimator
                .estimate_entries_tokens(&context_entries),
        );

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
                    |entry| self.transcript_estimator.estimate_entry_tokens(entry),
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

        let tokens_before = self
            .transcript_estimator
            .estimate_entries_tokens(context_entries);
        let tokens_after = self
            .transcript_estimator
            .estimate_entries_tokens(retained_entries);
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
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(session_id).cloned()
            }
            .ok_or_else(|| format!("missing storage path for session {session_id}"))?;
            let entry = storage
                .append_compaction(
                    &path,
                    parent_id.as_deref(),
                    &summary,
                    &first_kept_id,
                    None,
                    tokens_before,
                    Some(details),
                )
                .map_err(|error| error.to_string())?;
            state
                .append_entry(session_id, entry)
                .map_err(|error| error.to_string())?;
        } else {
            let entry = SessionTreeEntry::Compaction(crate::api::CompactionEntry {
                id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                parent_id,
                timestamp: crate::util::now_ms().to_string(),
                summary,
                first_kept_entry_id: first_kept_id,
                tokens_before,
                details: Some(details),
                from_hook: None,
            });
            state
                .append_entry(session_id, entry)
                .map_err(|error| error.to_string())?;
        }
        let session = state
            .session_mut(session_id)
            .map_err(|error| error.to_string())?;
        session.compaction.pending = false;
        session.compaction.window_number = window_number_before + 1;
        session.compaction.rearm_tokens = Some(tokens_after);
        session.world_state_baseline = None;
        drop(state);

        // H2: compact that rewrites the projected tree must rebuild via reconcile.
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
        Ok(())
    }
}
