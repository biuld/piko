use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub mod commands;
pub mod transport;

pub use transport::{run_jsonl_server, run_stdio_server};

use crate::api::{Command, ProtocolError, ServerMessage};
use llmd::gateway::LlmGateway;

use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::domain::config::ModelRegistry;
use crate::domain::sessions::HostState;
use crate::domain::turns::{MockTurnRunner, OrchTurnRunner, TurnRunner};
use crate::infra::storage::{JsonlSessionRepository, SessionStorageError};
use llmd::auth::AuthStorage;

use crate::domain::commands::command_catalog;
use crate::domain::config::HostSettings;

#[derive(Clone)]
pub struct HostServer {
    state: Arc<Mutex<HostState>>,
    storage: Option<JsonlSessionRepository>,
    session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    turn_runner: Arc<Mutex<Arc<dyn TurnRunner>>>,
    model_executor: Arc<Mutex<Option<Arc<dyn LlmGateway>>>>,
    settings: Arc<Mutex<HostSettings>>,
    model_registry: Arc<Mutex<ModelRegistry>>,
    project_settings_path: Arc<Mutex<Option<PathBuf>>>,
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
            turn_runner: Arc::new(Mutex::new(Arc::new(MockTurnRunner) as Arc<dyn TurnRunner>)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(HostSettings::default())),
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(
                AuthStorage::in_memory(std::collections::HashMap::new()),
                vec![],
            ))),
            project_settings_path: Arc::new(Mutex::new(None)),
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
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(
                AuthStorage::in_memory(std::collections::HashMap::new()),
                vec![],
            ))),
            project_settings_path: Arc::new(Mutex::new(None)),
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
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(
                AuthStorage::in_memory(std::collections::HashMap::new()),
                vec![],
            ))),
            project_settings_path: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_storage_runner_settings(
        storage: JsonlSessionRepository,
        turn_runner: Arc<dyn TurnRunner>,
        settings: HostSettings,
    ) -> Self {
        let auth = AuthStorage::create(None)
            .unwrap_or_else(|_| AuthStorage::in_memory(std::collections::HashMap::new()));
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: Some(storage),
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(settings)),
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(auth, vec![]))),
            project_settings_path: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the model executor (used for compaction and other host-level LLM calls).
    pub async fn set_model_executor(&self, executor: Arc<dyn LlmGateway>) {
        *self.model_executor.lock().await = Some(executor);
    }

    pub async fn handle_command(&self, command: Command) -> Vec<ServerMessage> {
        let mut rx = self.handle_command_stream(command);
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    pub fn handle_command_stream(&self, command: Command) -> UnboundedReceiver<ServerMessage> {
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
                    ServerMessage::CommandResponse {
                        command_id: command_id.clone(),
                        result: Err(err.to_string()),
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
        tx: &UnboundedSender<ServerMessage>,
    ) -> Result<(), ProtocolError> {
        match command {
            Command::AuthLoginOAuth { provider, .. } => {
                self.start_oauth_login(&command_id, provider, tx);
                Ok(())
            }
            Command::TurnSubmit {
                session_id, text, ..
            } => {
                self.apply_turn_submit(command_id, session_id, text, tx)
                    .await
            }
            Command::SessionCompact { session_id, .. } => {
                // Manual compaction — bypass threshold, always compact.
                self.compact_session_if_needed(&command_id, &session_id, 0, tx)
                    .await;
                Ok(())
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

    async fn apply_command(&self, command: Command) -> Result<Vec<ServerMessage>, ProtocolError> {
        let command_id = command.command_id().to_string();
        if let Command::ConfigUpdate { .. } = command {
            return self.apply_config_update(&command_id, command).await;
        }

        match command {
            Command::AuthLoginOAuth { .. } => unreachable!("auth oauth handled in stream"),
            Command::AuthSetApiKey {
                provider, api_key, ..
            } => {
                self.apply_auth_set_api_key(&command_id, provider, api_key)
                    .await
            }
            Command::AuthLogout { provider, .. } => {
                self.apply_auth_logout(&command_id, provider).await
            }
            Command::SessionCreate { cwd, .. } => self.apply_session_create(&command_id, cwd).await,
            Command::SessionOpen {
                session_id,
                session_path,
                ..
            } => {
                self.apply_session_open(&command_id, session_id, session_path)
                    .await
            }
            Command::SessionList { scope, cwd, .. } => {
                self.apply_session_list(&command_id, scope, cwd).await
            }
            Command::ModelList { .. } => {
                let registry = self.model_registry.lock().await;
                let providers = registry.list_providers();
                Ok(vec![ServerMessage::CommandResponse {
                    command_id: command_id.clone(),
                    result: Ok(crate::api::CommandResult::ModelListed {
                        providers,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::CommandCatalogGet { .. } => Ok(vec![ServerMessage::CommandResponse {
                command_id: command_id.clone(),
                result: Ok(crate::api::CommandResult::CommandCatalogListed {
                    commands: command_catalog(),
                    timestamp: now_ms(),
                }),
            }]),
            Command::SessionFork {
                session_id,
                entry_id,
                ..
            } => {
                self.apply_session_fork(&command_id, session_id, entry_id)
                    .await
            }
            Command::SessionImport { path, .. } => {
                self.apply_session_import(&command_id, path).await
            }
            Command::SessionRename {
                session_id, name, ..
            } => {
                self.apply_session_rename(&command_id, session_id, name)
                    .await
            }
            Command::SessionDelete { session_id, .. } => {
                self.apply_session_delete(&command_id, session_id).await
            }
            Command::SessionNavigate {
                session_id,
                entry_id,
                summarize,
                custom_instructions,
                ..
            } => {
                self.apply_session_navigate(
                    &command_id,
                    session_id,
                    entry_id,
                    summarize,
                    custom_instructions,
                )
                .await
            }
            Command::SessionSetLabel {
                session_id,
                entry_id,
                label,
                ..
            } => {
                self.apply_session_set_label(&command_id, session_id, entry_id, label)
                    .await
            }
            Command::StateSnapshot { session_id, .. }
            | Command::EventsResume { session_id, .. } => {
                self.apply_session_snapshot(&command_id, session_id).await
            }
            Command::QueueSteer {
                session_id,
                task_id,
                message,
                ..
            } => {
                let (queue_ev, has_active_turn) = {
                    let mut state = self.state.lock().await;
                    let queue_ev = state.push_steer(&session_id, &task_id, &message);
                    let has_active_turn = state
                        .session(&session_id)
                        .ok()
                        .and_then(|s| s.active_turn_id.clone())
                        .is_some();
                    (queue_ev, has_active_turn)
                };
                // Also route to the active orchd task if a turn is running
                if has_active_turn {
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
                let mut state = self.state.lock().await;
                let queue_ev = state.push_follow_up(&session_id, &message);
                Ok(vec![queue_ev.into()])
            }
            Command::QueueNextTurn {
                session_id,
                message,
                ..
            } => {
                let mut state = self.state.lock().await;
                let queue_ev = state.push_next_turn(&session_id, &message);
                Ok(vec![queue_ev.into()])
            }
            Command::TurnCancel {
                session_id,
                turn_id,
                ..
            } => {
                let mut state = self.state.lock().await;
                Ok(vec![state.cancel_turn(&session_id, &turn_id)?])
            }
            Command::ApprovalRespond {
                session_id,
                approval_id,
                decision,
                ..
            } => {
                self.turn_runner
                    .lock()
                    .await
                    .clone()
                    .respond_approval(&approval_id, decision.clone())
                    .await?;
                Ok(vec![ServerMessage::Approval(
                    crate::api::ApprovalEvent::Resolved {
                        task_id: session_id.clone(),
                        agent_id: "hostd".into(),
                        approval_id,
                        decision,
                    },
                )])
            }
            Command::UserInteractionRespond {
                session_id,
                interaction_id,
                response,
                ..
            } => {
                self.turn_runner
                    .lock()
                    .await
                    .clone()
                    .respond_user_interaction(&interaction_id, response.clone())
                    .await?;
                let status = match response {
                    crate::api::UserInteractionResponse::Submit { .. } => {
                        crate::api::UserInteractionStatus::Submitted
                    }
                    crate::api::UserInteractionResponse::Cancel { .. } => {
                        crate::api::UserInteractionStatus::Cancelled
                    }
                };
                Ok(vec![ServerMessage::Display(
                    piko_protocol::DisplayEvent::InteractionResolved {
                        task_id: session_id.clone(),
                        agent_id: "hostd".into(),
                        interaction_id,
                        status,
                    },
                )])
            }
            Command::TurnSubmit { .. } => Err(ProtocolError::InvalidCommand(
                "turn_submit requires streaming command handling".into(),
            )),
            Command::ConfigGet { namespace, .. } => {
                let settings = self.settings.lock().await;
                let value = match namespace.as_str() {
                    "tui" => settings
                        .tui
                        .clone()
                        .unwrap_or(serde_json::Value::Object(Default::default())),
                    _ => serde_json::Value::Object(Default::default()),
                };
                Ok(vec![ServerMessage::CommandResponse {
                    command_id: command_id.clone(),
                    result: Ok(crate::api::CommandResult::ConfigEntry { namespace, value }),
                }])
            }
            Command::ConfigUpdate { .. } => unreachable!("config_update handled before state lock"),
            Command::SessionCompact { .. } => {
                unreachable!("session_compact handled in streaming path")
            }
            Command::AgentList {
                session_id,
                command_id,
            } => {
                let state = self.state.lock().await;
                let agents = state.get_agent_list(&session_id);
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::AgentListed {
                        agents,
                        timestamp: now_ms(),
                    }),
                }])
            }
            Command::AgentSubscribe {
                session_id,
                agent_id,
                command_id,
            } => {
                let mut state = self.state.lock().await;
                state.set_active_agent(&session_id, &agent_id)?;
                Ok(vec![ServerMessage::CommandResponse {
                    command_id,
                    result: Ok(crate::api::CommandResult::AgentSubscribed { agent_id }),
                }])
            }
            Command::AgentUnsubscribe {
                agent_id: _,
                command_id,
                ..
            } => Ok(vec![ServerMessage::CommandResponse {
                command_id,
                result: Ok(crate::api::CommandResult::Empty),
            }]),
        }
    }
}

/// Build an OrchTurnRunner and return both the runner and the model executor (if available).
pub(super) async fn build_orch_turn_runner(
    settings: &HostSettings,
) -> Result<(Arc<dyn TurnRunner>, Option<Arc<dyn LlmGateway>>), String> {
    let mut auth = AuthStorage::create(None).map_err(|error| error.to_string())?;
    let registry = ModelRegistry::new(auth.clone(), vec![]);
    let resolved = registry
        .resolve(
            settings.default_model.as_deref(),
            settings.default_provider.as_deref(),
        )
        .ok_or_else(|| "no model available for hostd".to_string())?;

    let provider = &resolved.provider;
    let api_key = auth
        .resolve_oauth_api_key(provider)
        .await
        .map_err(|e| format!("failed to resolve auth for provider {provider}: {e}"))?
        .or_else(|| auth.get_api_key(provider))
        .ok_or_else(|| format!("no auth configured for provider {provider}"))?;

    let api_key_for_runner = api_key.clone();
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        resolved.provider.clone(),
        orchd::protocol::config::ProviderConfig {
            kind: resolved.provider.clone(),
            api_key,
            base_url: resolved.provider_config.base_url.clone(),
            headers: resolved.provider_config.headers.clone(),
        },
    );
    let retry_config = orchd::protocol::config::RetryConfig {
        enabled: settings
            .retry
            .as_ref()
            .and_then(|r| r.enabled)
            .unwrap_or(true),
        max_retries: settings
            .retry
            .as_ref()
            .and_then(|r| r.max_retries)
            .unwrap_or(3),
        base_delay_ms: settings
            .retry
            .as_ref()
            .and_then(|r| r.base_delay_ms)
            .unwrap_or(2000),
    };
    let executor = llmd::build_gateway(providers, retry_config);
    let thinking = settings.default_thinking_level.clone();
    let thinking_map = resolved.model.thinking_level_map.clone();
    let runner = Arc::new(
        OrchTurnRunner::new_with_mcp(
            executor.clone(),
            &resolved.provider,
            &api_key_for_runner,
            &resolved.model.id,
            thinking,
            thinking_map,
            &settings.mcp_servers,
            settings.sandbox.as_ref(),
        )
        .await,
    );
    Ok((runner, Some(executor)))
}

pub(super) fn send_event(tx: &UnboundedSender<ServerMessage>, event: ServerMessage) {
    let _ = tx.send(event);
}

pub(super) fn storage_error(error: SessionStorageError) -> ProtocolError {
    ProtocolError::InvalidCommand(error.to_string())
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
