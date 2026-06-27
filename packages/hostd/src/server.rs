use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::{
    Command, CommandAck, ContentBlock, Event, Message, MessageContent, MessageEntry, ProtocolError,
    SessionTreeEntry,
};
use piko_protocol::executor::LlmGateway;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use llmd::auth::AuthStorage;
use crate::compaction::{
    CompactionSettings, active_branch_entries, context_entries_after_compaction, should_compact,
};
use crate::models::ModelRegistry;
use crate::prompts::{
    BuildSystemPromptOptions, build_system_prompt, expand_prompt_template, load_context_files,
    load_prompt_templates,
};
use crate::session::{JsonlSessionRepository, SessionStorageError, load_session};
use crate::settings::{HostSettings, SettingsManager};
use crate::skills::load_skills;
use crate::state::HostState;
use crate::turn_runner::{
    ErrorTurnRunner, MockTurnRunner, OrchTurnRunner, TurnRunInput, TurnRunner,
};

#[derive(Clone)]
pub struct HostServer {
    state: Arc<Mutex<HostState>>,
    storage: Option<JsonlSessionRepository>,
    session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    turn_runner: Arc<Mutex<Arc<dyn TurnRunner>>>,
    model_executor: Arc<Mutex<Option<Arc<dyn LlmGateway>>>>,
    settings: Arc<Mutex<HostSettings>>,
}

impl Default for HostServer {
    fn default() -> Self {
        Self::new()
    }
}

impl HostServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: None,
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(Arc::new(MockTurnRunner))),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(HostSettings::default())),
        }
    }

    pub fn with_storage(storage: JsonlSessionRepository) -> Self {
        Self::with_storage_and_runner(storage, Arc::new(MockTurnRunner))
    }

    pub fn with_turn_runner(turn_runner: Arc<dyn TurnRunner>) -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: None,
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(HostSettings::default())),
        }
    }

    pub fn with_storage_and_runner(
        storage: JsonlSessionRepository,
        turn_runner: Arc<dyn TurnRunner>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: Some(storage),
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(HostSettings::default())),
        }
    }

    pub fn with_storage_runner_settings(
        storage: JsonlSessionRepository,
        turn_runner: Arc<dyn TurnRunner>,
        settings: HostSettings,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: Some(storage),
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(settings)),
        }
    }

    /// Set the model executor (used for compaction and other host-level LLM calls).
    pub async fn set_model_executor(&self, executor: Arc<dyn LlmGateway>) {
        *self.model_executor.lock().await = Some(executor);
    }

    pub async fn handle_command(&self, command: Command) -> Vec<Event> {
        let mut rx = self.handle_command_stream(command);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    pub fn handle_command_stream(&self, command: Command) -> UnboundedReceiver<Event> {
        let command_id = command.command_id().to_string();
        let server = self.clone();
        let (tx, rx) = unbounded_channel();
        tokio::spawn(async move {
            if let Err(err) = server
                .apply_command_stream(command, command_id.clone(), &tx)
                .await
            {
                send_event(
                    &tx,
                    Event::TaskFailed {
                        session_id: String::new(),
                        task_id: command_id.clone(),
                        agent_id: "hostd".into(),
                        error: err.to_string(),
                        timestamp: now_ms(),
                    },
                );
            }
        });
        rx
    }

    async fn apply_command_stream(
        &self,
        command: Command,
        command_id: String,
        tx: &UnboundedSender<Event>,
    ) -> Result<(), ProtocolError> {
        match command {
            Command::AuthLoginStart { provider, .. } => {
                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    if provider == "openai" {
                        use llmd::providers::OAuthProvider;
                        let oauth = llmd::providers::openai::OpenAIOAuth::new();
                        match oauth.start_device_auth().await {
                            Ok(info) => {
                                let _ = tx_clone.send(Event::AuthLoginDeviceCode {
                                    provider: provider.clone(),
                                    user_code: info.user_code.clone(),
                                    verification_uri: info.verification_uri.clone(),
                                });

                                match oauth.poll_device_auth(&info).await {
                                    Ok((code, verifier)) => {
                                        match oauth.exchange_code(code, verifier).await {
                                            Ok(_cred) => {
                                                let _ = tx_clone.send(Event::AuthLoginSuccess {
                                                    provider: provider.clone(),
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx_clone.send(Event::AuthLoginFailed {
                                                    provider: provider.clone(),
                                                    error: format!("Exchange failed: {e}"),
                                                });
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx_clone.send(Event::AuthLoginFailed {
                                            provider: provider.clone(),
                                            error: format!("Poll failed: {e}"),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx_clone.send(Event::AuthLoginFailed {
                                    provider: provider.clone(),
                                    error: format!("Start failed: {e}"),
                                });
                            }
                        }
                    } else {
                        let _ = tx_clone.send(Event::AuthLoginFailed {
                            provider,
                            error: "Only openai device code auth is currently supported".into(),
                        });
                    }
                });
                Ok(())
            }
            Command::TurnSubmit {
                session_id, text, ..
            } => {
                self.apply_turn_submit(command_id, session_id, text, tx)
                    .await
            }
            command => {
                let events = self.apply_command(command).await?;
                for event in events {
                    send_event(tx, event);
                }
                Ok(())
            }
        }
    }

    async fn apply_command(&self, command: Command) -> Result<Vec<Event>, ProtocolError> {
        if let Command::ConfigSet {
            default_provider,
            default_model,
            default_thinking_level,
            active_tools,
            ..
        } = command
        {
            let mut settings = self.settings.lock().await;
            if default_provider.is_some() {
                settings.default_provider = default_provider;
            }
            if default_model.is_some() {
                settings.default_model = default_model;
            }
            if default_thinking_level.is_some() {
                settings.default_thinking_level = default_thinking_level;
            }
            if active_tools.is_some() {
                settings.active_tool_names = active_tools;
            }
            let model_id = settings.default_model.clone().unwrap_or_default();
            let provider = settings.default_provider.clone().unwrap_or_default();
            let thinking_level = settings.default_thinking_level.clone();
            let (runner, executor) = build_orch_turn_runner(&settings).await.unwrap_or_else(|e| {
                (
                    Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>,
                    None,
                )
            });
            *self.turn_runner.lock().await = runner;
            if let Some(exec) = executor {
                self.set_model_executor(exec).await;
            }

            // Persist config metadata to journal for each active session
            if let Some(storage) = &self.storage {
                let paths = self.session_paths.lock().await;
                for (session_id, path) in paths.iter() {
                    let parent_id = {
                        let state = self.state.lock().await;
                        state
                            .session(session_id)
                            .ok()
                            .and_then(|s| s.current_leaf_id.clone())
                    };
                    if let Err(e) = storage.append_config_metadata(
                        path,
                        parent_id.as_deref(),
                        if model_id.is_empty() {
                            None
                        } else {
                            Some(&model_id)
                        },
                        if provider.is_empty() {
                            None
                        } else {
                            Some(&provider)
                        },
                        thinking_level.as_deref(),
                    ) {
                        tracing::warn!(
                            "Failed to persist config metadata for session {session_id}: {e}"
                        );
                    }
                }
            }

            // Emit ModelConfigChanged for each active session
            let state = self.state.lock().await;
            let ts = now_ms();
            let events: Vec<Event> = state
                .list_sessions()
                .into_iter()
                .map(|s| Event::ModelConfigChanged {
                    session_id: s.session_id,
                    model_id: model_id.clone(),
                    provider: provider.clone(),
                    timestamp: ts,
                })
                .collect();
            return Ok(events);
        }

        let mut state = self.state.lock().await;
        match command {
            Command::AuthLoginStart { .. } => unreachable!("auth handled in stream"),
            Command::SessionCreate { cwd, .. } => {
                if let Some(storage) = &self.storage {
                    let persisted = storage.create(&cwd).map_err(storage_error)?;
                    let session_id = persisted.state.session_id.clone();
                    self.session_paths
                        .lock()
                        .await
                        .insert(session_id.clone(), persisted.path);
                    state.insert_session(persisted.state);
                    Ok(vec![Event::SessionCreated {
                        session_id,
                        cwd,
                        timestamp: now_ms(),
                    }])
                } else {
                    Ok(vec![state.create_session(cwd)])
                }
            }
            Command::SessionOpen { session_id, .. } => {
                if !state.has_session(&session_id) {
                    if let Some(storage) = &self.storage {
                        let cwd = std::env::current_dir()
                            .ok()
                            .and_then(|cwd| cwd.to_str().map(str::to_string))
                            .unwrap_or_else(|| ".".to_string());
                        let persisted = storage.open(&cwd, &session_id).map_err(storage_error)?;
                        let opened_id = persisted.state.session_id.clone();
                        self.session_paths
                            .lock()
                            .await
                            .insert(opened_id.clone(), persisted.path);
                        state.insert_session(persisted.state);
                        let snapshot = state.snapshot(&opened_id)?;
                        return Ok(vec![Event::SessionOpened {
                            session_id: opened_id,
                            snapshot,
                            timestamp: now_ms(),
                        }]);
                    }
                    return Err(ProtocolError::SessionNotFound(session_id));
                }
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![Event::SessionOpened {
                    session_id: session_id.clone(),
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            Command::SessionList { .. } => {
                let sessions = if let Some(storage) = &self.storage {
                    storage.summaries(None).map_err(storage_error)?
                } else {
                    state.list_sessions()
                };
                Ok(vec![Event::SessionListed {
                    sessions,
                    timestamp: now_ms(),
                }])
            }
            Command::SessionFork {
                session_id,
                entry_id,
                ..
            } => {
                if let Some(storage) = &self.storage {
                    let source_path = {
                        let paths = self.session_paths.lock().await;
                        paths.get(&session_id).cloned()
                    };
                    let Some(source_path) = source_path else {
                        return Err(ProtocolError::SessionNotFound(session_id));
                    };
                    let persisted = storage
                        .fork(&session_id, &source_path, entry_id.as_deref())
                        .map_err(storage_error)?;
                    let forked_id = persisted.state.session_id.clone();
                    self.session_paths
                        .lock()
                        .await
                        .insert(forked_id.clone(), persisted.path);
                    state.insert_session(persisted.state);
                    let snapshot = state.snapshot(&forked_id)?;
                    Ok(vec![
                        Event::SessionCreated {
                            session_id: forked_id.clone(),
                            cwd: snapshot.cwd.clone(),
                            timestamp: now_ms(),
                        },
                        Event::SessionOpened {
                            session_id: forked_id,
                            snapshot,
                            timestamp: now_ms(),
                        },
                    ])
                } else {
                    Err(ProtocolError::InvalidCommand(
                        "session_fork requires persistent storage".into(),
                    ))
                }
            }
            Command::SessionImport { path, .. } => {
                let Some(storage) = &self.storage else {
                    return Err(ProtocolError::InvalidCommand(
                        "session_import requires persistent storage".into(),
                    ));
                };
                let persisted = storage
                    .import(&PathBuf::from(path))
                    .map_err(storage_error)?;
                let imported_id = persisted.state.session_id.clone();
                self.session_paths
                    .lock()
                    .await
                    .insert(imported_id.clone(), persisted.path);
                state.insert_session(persisted.state);
                let snapshot = state.snapshot(&imported_id)?;
                Ok(vec![
                    Event::SessionCreated {
                        session_id: imported_id.clone(),
                        cwd: snapshot.cwd.clone(),
                        timestamp: now_ms(),
                    },
                    Event::SessionOpened {
                        session_id: imported_id,
                        snapshot,
                        timestamp: now_ms(),
                    },
                ])
            }
            Command::SessionRename {
                session_id, name, ..
            } => {
                let session = state.session_mut(&session_id)?;
                session.name = Some(name.clone());
                if let Some(storage) = &self.storage {
                    let path = {
                        let paths = self.session_paths.lock().await;
                        paths.get(&session_id).cloned()
                    };
                    if let Some(path) = path {
                        storage
                            .append_session_info(&path, session.current_leaf_id.as_deref(), &name)
                            .map_err(storage_error)?;
                    }
                }
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![Event::SessionOpened {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            Command::SessionDelete { session_id, .. } => {
                state.delete_session(&session_id);
                let path = self.session_paths.lock().await.remove(&session_id);
                if let Some(path) = path {
                    let _ = std::fs::remove_file(path);
                }
                let sessions = if let Some(storage) = &self.storage {
                    storage.summaries(None).map_err(storage_error)?
                } else {
                    state.list_sessions()
                };
                Ok(vec![Event::SessionListed {
                    sessions,
                    timestamp: now_ms(),
                }])
            }
            Command::SessionNavigate {
                session_id,
                entry_id,
                ..
            } => {
                if let Some(storage) = &self.storage {
                    let path = {
                        let paths = self.session_paths.lock().await;
                        paths.get(&session_id).cloned()
                    };
                    if let Some(path) = path {
                        let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                        storage
                            .navigate(&path, parent_id.as_deref(), &entry_id)
                            .map_err(storage_error)?;
                        let persisted = load_session(&path).map_err(storage_error)?;
                        state.insert_session(persisted.state);
                    }
                }
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![Event::SessionOpened {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            Command::StateSnapshot { session_id, .. }
            | Command::EventsResume { session_id, .. } => {
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![Event::StateSnapshot {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            Command::QueueSteer {
                session_id,
                task_id,
                message,
                ..
            } => {
                let queue_ev = state.push_steer(&session_id, &task_id, &message);
                // Also route to the active orchd task if a turn is running
                if state
                    .session(&session_id)
                    .ok()
                    .and_then(|s| s.active_turn_id.clone())
                    .is_some()
                {
                    let runner = self.turn_runner.lock().await.clone();
                    let _ = runner
                        .steer_task(&task_id, "queue", "hostd", &message)
                        .await;
                }
                Ok(vec![queue_ev.into()])
            }
            Command::QueueFollowUp {
                session_id,
                message,
                ..
            } => {
                let queue_ev = state.push_follow_up(&session_id, &message);
                Ok(vec![queue_ev.into()])
            }
            Command::QueueNextTurn {
                session_id,
                message,
                ..
            } => {
                let queue_ev = state.push_next_turn(&session_id, &message);
                Ok(vec![queue_ev.into()])
            }
            Command::TurnCancel {
                session_id,
                turn_id,
                ..
            } => Ok(vec![state.cancel_turn(&session_id, &turn_id)?]),
            Command::ApprovalRespond {
                session_id,
                approval_id,
                decision,
                ..
            } => {
                let runner = self.turn_runner.lock().await.clone();
                runner
                    .respond_approval(&approval_id, decision.clone())
                    .await?;
                Ok(vec![Event::ApprovalResolved {
                    task_id: session_id.clone(),
                    agent_id: "hostd".into(),
                    approval_id,
                    decision,
                }])
            }
            Command::TurnSubmit { .. } => Err(ProtocolError::InvalidCommand(
                "turn_submit requires streaming command handling".into(),
            )),
            Command::ConfigSet { .. } => unreachable!("config_set handled before state lock"),
        }
    }

    /// Check if the active branch exceeds the compaction threshold and, if so,
    /// append a compaction entry to the session tree.
    async fn compact_session_if_needed(
        &self,
        session_id: &str,
        context_window: u64,
        tx: &UnboundedSender<Event>,
    ) {
        let c_settings;
        let enabled;
        {
            let settings = self.settings.lock().await;
            let (e, _, _) = (
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.enabled)
                    .unwrap_or(true),
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.reserve_tokens)
                    .unwrap_or(16384),
                settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000),
            );
            c_settings = CompactionSettings {
                enabled: e,
                reserve_tokens: settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.reserve_tokens)
                    .unwrap_or(16384),
                keep_recent_tokens: settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000),
            };
            enabled = e;
        }
        if !enabled {
            return;
        }

        let state_lock = self.state.lock().await;
        let branch_entries = state_lock
            .session(session_id)
            .map(|session| {
                active_branch_entries(&session.entries, session.current_leaf_id.as_deref())
            })
            .unwrap_or_default();
        drop(state_lock);

        let context_entries = context_entries_after_compaction(&branch_entries);

        if !should_compact(&context_entries, context_window, &c_settings) {
            return;
        }

        // Find cut point
        let cut_point = crate::compaction::find_cut_point(
            &context_entries,
            0,
            context_entries.len(),
            c_settings.keep_recent_tokens,
        );
        let cut_index = cut_point.first_kept_entry_index;

        if cut_index == 0 {
            return;
        }

        let entries_to_summarize = &context_entries[0..cut_index];
        let retained_entries = &context_entries[cut_index..];

        let previous_summary = entries_to_summarize
            .iter()
            .rev()
            .find_map(|entry| match entry {
                SessionTreeEntry::Compaction(compaction) => Some(compaction.summary.as_str()),
                _ => None,
            });

        // Try LLM summarization
        let summary = {
            let executor_guard = self.model_executor.lock().await;
            if let Some(ref executor) = *executor_guard {
                let (model_id, provider) = {
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

                let model = orchd::protocol::messages::Model {
                    id: model_id.clone(),
                    name: model_id,
                    provider,
                    base_url: None,
                };

                crate::compaction::summarizer::summarize_history(
                    executor.clone(),
                    model,
                    entries_to_summarize,
                    previous_summary,
                    "",
                )
                .await
                .ok()
            } else {
                None
            }
        };

        if let Some(summary) = summary {
            let first_kept_id = retained_entries
                .first()
                .map(|entry| entry.id().to_string())
                .unwrap_or_default();

            let mut state = self.state.lock().await;
            let parent_id = state
                .session(session_id)
                .ok()
                .and_then(|session| session.current_leaf_id.clone());

            // Persist compaction metadata
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
                    )
                {
                    let _ = state.append_entry(session_id, entry);
                }
            }

            // Emit a state snapshot so TUI knows transcript changed
            let snapshot = state.snapshot(session_id).ok();
            drop(state);

            if let Some(snapshot) = snapshot {
                send_event(
                    tx,
                    Event::StateSnapshot {
                        session_id: session_id.to_string(),
                        snapshot,
                        timestamp: now_ms(),
                    },
                );
            }
        }
    }

    async fn apply_turn_submit(
        &self,
        _command_id: String,
        session_id: String,
        text: String,
        tx: &UnboundedSender<Event>,
    ) -> Result<(), ProtocolError> {
        let mut state = self.state.lock().await;
        let cwd = state.session_cwd(&session_id)?;
        let templates = load_prompt_templates(&cwd);
        let expanded_text = expand_prompt_template(&text, &templates);
        let context_files = load_context_files(&cwd);
        let skills = load_skills(&cwd).skills;
        let system_prompt = build_system_prompt(BuildSystemPromptOptions {
            cwd: PathBuf::from(&cwd),
            context_files,
            skills,
            prompt_templates: templates,
            ..BuildSystemPromptOptions::default()
        });

        let (turn_id, start_events) = state.start_turn(&session_id)?;
        for event in start_events {
            send_event(tx, event);
        }

        let user_message = Message::User {
            content: MessageContent::String(expanded_text.clone()),
            timestamp: Some(now_ms()),
        };
        let user_entry = if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                storage
                    .append_message(&path, parent_id.as_deref(), &user_message)
                    .map_err(storage_error)?
            } else {
                SessionTreeEntry::Message(MessageEntry {
                    id: format!("msg_{}", uuid::Uuid::new_v4()),
                    parent_id: state.session(&session_id)?.current_leaf_id.clone(),
                    timestamp: now_ms().to_string(),
                    message: user_message,
                })
            }
        } else {
            SessionTreeEntry::Message(MessageEntry {
                id: format!("msg_{}", uuid::Uuid::new_v4()),
                parent_id: state.session(&session_id)?.current_leaf_id.clone(),
                timestamp: now_ms().to_string(),
                message: user_message,
            })
        };
        let user_message_id = user_entry.id().to_string();
        state.append_entry(&session_id, user_entry)?;

        // Persist turn config metadata (model, provider, thinking level)
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let settings = self.settings.lock().await;
                let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                if let Ok(entries) = storage.append_config_metadata(
                    &path,
                    parent_id.as_deref(),
                    settings.default_model.as_deref(),
                    settings.default_provider.as_deref(),
                    settings.default_thinking_level.as_deref(),
                ) {
                    for entry in entries {
                        let _ = state.append_entry(&session_id, entry);
                    }
                }
            }
        }

        send_event(
            tx,
            Event::UserMessageSubmitted {
                session_id: session_id.clone(),
                message_id: user_message_id,
                task_id: turn_id.clone(),
                text: expanded_text.clone(),
                timestamp: now_ms(),
            },
        );

        let active_tool_names = self.settings.lock().await.active_tool_names.clone();
        let runner = self.turn_runner.lock().await.clone();
        let output = runner
            .run_turn(
                TurnRunInput {
                    session_id: session_id.clone(),
                    turn_id,
                    prompt: expanded_text,
                    system_prompt,
                    active_tool_names,
                },
                &mut state,
                Some(tx.clone()),
            )
            .await?;
        let session_path = if self.storage.is_some() {
            let paths = self.session_paths.lock().await;
            paths.get(&session_id).cloned()
        } else {
            None
        };
        for event in output.events {
            persist_completed_message_event(
                &self.storage,
                session_path.as_ref(),
                &mut state,
                &session_id,
                &event,
            )?;
            send_event(tx, event);
        }
        drop(state);

        // Check if compaction is needed after turn completes
        let context_window = {
            let settings = self.settings.lock().await;
            settings
                .compaction
                .as_ref()
                .and_then(|c| c.reserve_tokens)
                .unwrap_or(16384)
                + settings
                    .compaction
                    .as_ref()
                    .and_then(|c| c.keep_recent_tokens)
                    .unwrap_or(20000)
        };
        self.compact_session_if_needed(&session_id, context_window, tx)
            .await;

        // Collect all pending follow-up / next-turn items
        let mut queued: Vec<String> = Vec::new();
        let mut state = self.state.lock().await;
        while let Some(next_text) = drain_one_queued(&mut state, &session_id) {
            queued.push(next_text);
        }
        drop(state);

        // Run turns for each queued prompt
        for next_text in queued {
            // Emit queue update before each
            {
                let s = self.state.lock().await;
                let qev: Event = s.build_queue_update(&session_id).into();
                drop(s);
                send_event(tx, qev);
            }
            Box::pin(self.apply_turn_submit(
                format!("auto-{}", uuid::Uuid::new_v4()),
                session_id.clone(),
                next_text,
                tx,
            ))
            .await?;
        }
        Ok(())
    }
}

pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let cwd = std::env::current_dir()?;
    let settings = SettingsManager::create(&cwd)?;
    let session_root = settings
        .settings()
        .session_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(JsonlSessionRepository::default_root);
    let (turn_runner, model_executor) = build_orch_turn_runner(&settings.settings())
        .await
        .unwrap_or_else(|e| {
            (
                Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>,
                None,
            )
        });
    let server = HostServer::with_storage_runner_settings(
        JsonlSessionRepository::new(session_root),
        turn_runner,
        settings.settings(),
    );
    if let Some(executor) = model_executor {
        server.set_model_executor(executor).await;
    }
    run_jsonl_server(BufReader::new(stdin), stdout, server).await
}

/// Build an OrchTurnRunner and return both the runner and the model executor (if available).
async fn build_orch_turn_runner(
    settings: &HostSettings,
) -> Result<(Arc<dyn TurnRunner>, Option<Arc<dyn LlmGateway>>), String> {
    let mut auth = AuthStorage::create(None).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::new(
        auth.clone(),
        settings.enabled_models.clone().unwrap_or_default(),
    );
    let resolved = registry
        .resolve(
            settings.default_model.as_deref(),
            settings.default_provider.as_deref(),
        )
        .ok_or_else(|| "no model available for hostd".to_string())?;

    let provider = &resolved.model.provider;
    let api_key = auth
        .resolve_oauth_api_key(provider)
        .await
        .map_err(|e| format!("failed to resolve auth for provider {provider}: {e}"))?
        .or_else(|| auth.get_api_key(provider))
        .ok_or_else(|| format!("no auth configured for provider {provider}"))?;

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        resolved.model.provider.clone(),
        orchd::protocol::config::ProviderConfig {
            kind: resolved.model.provider.clone(),
            api_key,
            base_url: resolved.provider_config.base_url.clone(),
            headers: resolved.provider_config.headers.clone(),
        },
    );
    let executor = llmd::build_gateway(providers);
    let runner =
        Arc::new(OrchTurnRunner::new_with_mcp(executor.clone(), &settings.mcp_servers).await);
    Ok((runner, Some(executor)))
}

pub async fn run_jsonl_server<R, W>(
    mut reader: R,
    mut writer: W,
    server: HostServer,
) -> Result<(), Box<dyn std::error::Error>>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<Command>(line.trim());
        let mut events = match parsed {
            Ok(command) => {
                let command_id = command.command_id().to_string();
                write_ack(&mut writer, CommandAck::CommandAccepted { command_id }).await?;
                server.handle_command_stream(command)
            }
            Err(err) => {
                let (_tx, rx) = unbounded_channel();
                write_ack(
                    &mut writer,
                    CommandAck::CommandRejected {
                        command_id: "unknown".to_string(),
                        reason: format!("invalid json command: {err}"),
                    },
                )
                .await?;
                rx
            }
        };
        while let Some(event) = events.recv().await {
            let encoded = serde_json::to_string(&event)?;
            writer.write_all(encoded.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }
    Ok(())
}

fn send_event(tx: &UnboundedSender<Event>, event: Event) {
    let _ = tx.send(event);
}

async fn write_ack<W>(writer: &mut W, ack: CommandAck) -> Result<(), Box<dyn std::error::Error>>
where
    W: AsyncWrite + Unpin,
{
    let encoded = serde_json::to_string(&ack)?;
    writer.write_all(encoded.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

fn storage_error(error: SessionStorageError) -> ProtocolError {
    ProtocolError::InvalidCommand(error.to_string())
}

fn persist_completed_message_event(
    storage: &Option<JsonlSessionRepository>,
    session_path: Option<&PathBuf>,
    state: &mut HostState,
    session_id: &str,
    event: &Event,
) -> Result<(), ProtocolError> {
    let Some(entry) = completed_message_event_to_entry(state, session_id, event)? else {
        return Ok(());
    };

    if let (Some(storage), Some(path)) = (storage, session_path) {
        storage.append_entry(path, &entry).map_err(storage_error)?;
    }
    state.append_entry(session_id, entry)
}

fn completed_message_event_to_entry(
    state: &HostState,
    session_id: &str,
    event: &Event,
) -> Result<Option<SessionTreeEntry>, ProtocolError> {
    let parent_id = state.session(session_id)?.current_leaf_id.clone();
    let entry = match event {
        Event::AssistantMessageCompleted {
            message_id,
            text,
            tool_calls,
            model,
            provider,
            usage,
            timestamp,
            ..
        } => {
            let mut content = Vec::new();
            if !text.is_empty() {
                content.push(ContentBlock::Text { text: text.clone() });
            }
            content.extend(tool_calls.iter().map(|tool_call| ContentBlock::ToolCall {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments: tool_call.args.clone(),
                partial_json: None,
            }));
            SessionTreeEntry::Message(MessageEntry {
                id: message_id.clone(),
                parent_id,
                timestamp: timestamp.to_string(),
                message: Message::Assistant {
                    content,
                    api: "hostd".to_string(),
                    provider: provider.clone(),
                    model: model.clone(),
                    usage: usage.clone(),
                    stop_reason: None,
                    error_message: None,
                    timestamp: Some(*timestamp),
                },
            })
        }
        Event::ToolResultCommitted {
            message_id,
            tool_call_id,
            tool_name,
            content,
            is_error,
            timestamp,
            ..
        } => SessionTreeEntry::Message(MessageEntry {
            id: message_id.clone(),
            parent_id,
            timestamp: timestamp.to_string(),
            message: Message::ToolResult {
                tool_call_id: tool_call_id.clone(),
                tool_name: Some(tool_name.clone()),
                content: vec![ContentBlock::Text {
                    text: serde_json::to_string_pretty(content)
                        .unwrap_or_else(|_| content.to_string()),
                }],
                details: Some(content.clone()),
                is_error: Some(*is_error),
                timestamp: Some(*timestamp),
            },
        }),
        _ => return Ok(None),
    };
    Ok(Some(entry))
}

/// Drain one pending item from follow-up or next-turn queues.
/// Returns the prompt text or None if both queues are empty.
fn drain_one_queued(state: &mut HostState, session_id: &str) -> Option<String> {
    state
        .drain_next_follow_up(session_id)
        .or_else(|| state.drain_next_next_turn(session_id))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
