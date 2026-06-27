use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::api::{CommandAck, HostCommand, HostEvent, HostMessage, HostProtocolError, MessageRole};
use orchd::model::executor::SelfLlmExecutor;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::auth::AuthStorage;
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
    settings: Arc<Mutex<HostSettings>>,
}

impl HostServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: None,
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(Arc::new(MockTurnRunner))),
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
            settings: Arc::new(Mutex::new(settings)),
        }
    }

    pub async fn handle_command(&self, command: HostCommand) -> Vec<HostEvent> {
        let mut rx = self.handle_command_stream(command);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    pub fn handle_command_stream(&self, command: HostCommand) -> UnboundedReceiver<HostEvent> {
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
                    HostEvent::TaskFailed {
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
        command: HostCommand,
        command_id: String,
        tx: &UnboundedSender<HostEvent>,
    ) -> Result<(), HostProtocolError> {
        match command {
            HostCommand::TurnSubmit {
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

    async fn apply_command(
        &self,
        command: HostCommand,
    ) -> Result<Vec<HostEvent>, HostProtocolError> {
        if let HostCommand::ConfigSet {
            default_provider,
            default_model,
            default_thinking_level,
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
            let runner = build_orch_turn_runner(&settings)
                .await
                .unwrap_or_else(|e| Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>);
            *self.turn_runner.lock().await = runner;
            return Ok(vec![]);
        }

        let mut state = self.state.lock().await;
        match command {
            HostCommand::SessionCreate { cwd, .. } => {
                if let Some(storage) = &self.storage {
                    let persisted = storage.create(&cwd).map_err(storage_error)?;
                    let session_id = persisted.state.session_id.clone();
                    self.session_paths
                        .lock()
                        .await
                        .insert(session_id.clone(), persisted.path);
                    state.insert_session(persisted.state);
                    Ok(vec![HostEvent::SessionCreated {
                        session_id,
                        cwd,
                        timestamp: now_ms(),
                    }])
                } else {
                    Ok(vec![state.create_session(cwd)])
                }
            }
            HostCommand::SessionOpen { session_id, .. } => {
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
                        return Ok(vec![HostEvent::SessionOpened {
                            session_id: opened_id,
                            snapshot,
                            timestamp: now_ms(),
                        }]);
                    }
                    return Err(HostProtocolError::SessionNotFound(session_id));
                }
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![HostEvent::SessionOpened {
                    session_id: session_id.clone(),
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::SessionList { .. } => {
                let sessions = if let Some(storage) = &self.storage {
                    storage.summaries(None).map_err(storage_error)?
                } else {
                    state.list_sessions()
                };
                Ok(vec![HostEvent::SessionListed {
                    sessions,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::SessionFork {
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
                        return Err(HostProtocolError::SessionNotFound(session_id));
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
                        HostEvent::SessionCreated {
                            session_id: forked_id.clone(),
                            cwd: snapshot.cwd.clone(),
                            timestamp: now_ms(),
                        },
                        HostEvent::SessionOpened {
                            session_id: forked_id,
                            snapshot,
                            timestamp: now_ms(),
                        },
                    ])
                } else {
                    Err(HostProtocolError::InvalidCommand(
                        "session_fork requires persistent storage".into(),
                    ))
                }
            }
            HostCommand::SessionImport { path, .. } => {
                let Some(storage) = &self.storage else {
                    return Err(HostProtocolError::InvalidCommand(
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
                    HostEvent::SessionCreated {
                        session_id: imported_id.clone(),
                        cwd: snapshot.cwd.clone(),
                        timestamp: now_ms(),
                    },
                    HostEvent::SessionOpened {
                        session_id: imported_id,
                        snapshot,
                        timestamp: now_ms(),
                    },
                ])
            }
            HostCommand::SessionRename {
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
                Ok(vec![HostEvent::SessionOpened {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::SessionDelete { session_id, .. } => {
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
                Ok(vec![HostEvent::SessionListed {
                    sessions,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::SessionNavigate {
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
                Ok(vec![HostEvent::SessionOpened {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::StateSnapshot { session_id, .. }
            | HostCommand::EventsResume { session_id, .. } => {
                let snapshot = state.snapshot(&session_id)?;
                Ok(vec![HostEvent::StateSnapshot {
                    session_id,
                    snapshot,
                    timestamp: now_ms(),
                }])
            }
            HostCommand::TurnCancel {
                session_id,
                turn_id,
                ..
            } => Ok(vec![state.cancel_turn(&session_id, &turn_id)?]),
            HostCommand::ApprovalRespond {
                session_id,
                approval_id,
                decision,
                ..
            } => {
                let runner = self.turn_runner.lock().await.clone();
                runner
                    .respond_approval(&approval_id, decision.clone())
                    .await?;
                Ok(vec![HostEvent::ApprovalResolved {
                    task_id: session_id.clone(),
                    agent_id: "hostd".into(),
                    approval_id,
                    decision,
                }])
            }
            HostCommand::TurnSubmit { .. } => Err(HostProtocolError::InvalidCommand(
                "turn_submit requires streaming command handling".into(),
            )),
            HostCommand::ConfigSet { .. } => unreachable!("config_set handled before state lock"),
        }
    }

    async fn apply_turn_submit(
        &self,
        _command_id: String,
        session_id: String,
        text: String,
        tx: &UnboundedSender<HostEvent>,
    ) -> Result<(), HostProtocolError> {
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

        let mut user_message = HostMessage {
            id: format!("msg_{}", uuid::Uuid::new_v4()),
            role: MessageRole::User,
            text: expanded_text.clone(),
        };
        if let Some(storage) = &self.storage {
            let path = {
                let paths = self.session_paths.lock().await;
                paths.get(&session_id).cloned()
            };
            if let Some(path) = path {
                let parent_id = state.session(&session_id)?.current_leaf_id.clone();
                user_message.id = storage
                    .append_message(&path, parent_id.as_deref(), &user_message)
                    .map_err(storage_error)?;
            }
        }
        state.add_message(&session_id, user_message.clone())?;

        send_event(
            tx,
            HostEvent::UserMessageSubmitted {
                session_id: session_id.clone(),
                message_id: user_message.id.clone(),
                task_id: turn_id.clone(),
                text: user_message.text.clone(),
                timestamp: now_ms(),
            },
        );

        let runner = self.turn_runner.lock().await.clone();
        let output = runner
            .run_turn(
                TurnRunInput {
                    session_id: session_id.clone(),
                    turn_id,
                    prompt: expanded_text,
                    system_prompt,
                },
                &mut state,
                Some(tx.clone()),
            )
            .await?;
        for event in output.events {
            send_event(tx, event);
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
    let turn_runner = build_orch_turn_runner(&settings.settings())
        .await
        .unwrap_or_else(|e| Arc::new(ErrorTurnRunner::new(e)) as Arc<dyn TurnRunner>);
    run_jsonl_server(
        BufReader::new(stdin),
        stdout,
        HostServer::with_storage_runner_settings(
            JsonlSessionRepository::new(session_root),
            turn_runner,
            settings.settings(),
        ),
    )
    .await
}

async fn build_orch_turn_runner(settings: &HostSettings) -> Result<Arc<dyn TurnRunner>, String> {
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
    let executor = Arc::new(SelfLlmExecutor::from_providers(providers));
    Ok(Arc::new(OrchTurnRunner::new(executor).await))
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
        let parsed = serde_json::from_str::<HostCommand>(line.trim());
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

fn send_event(tx: &UnboundedSender<HostEvent>, event: HostEvent) {
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

fn storage_error(error: SessionStorageError) -> HostProtocolError {
    HostProtocolError::InvalidCommand(error.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
