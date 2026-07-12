use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use llmd::auth::AuthStorage;
use llmd::gateway::LlmGateway;
use tokio::sync::Mutex;

use crate::domain::config::{HostSettings, ModelRegistry};
use crate::domain::sessions::HostState;
use crate::infra::storage::JsonlSessionRepository;
use crate::ports::{ErrorTurnRunner, TurnRunner};

/// Application-layer composition root.
///
/// Owns all mutable host state plus the ports (turn runner, model executor,
/// storage) that use-cases in `application::{turns, sessions, compaction}`
/// orchestrate. `protocol::HostServer` is a thin newtype wrapper around this
/// type: command routing/transport lives in `protocol`, use-case bodies live
/// here.
#[derive(Clone)]
pub struct HostApp {
    pub(crate) state: Arc<Mutex<HostState>>,
    pub(crate) storage: Option<JsonlSessionRepository>,
    pub(crate) session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub(crate) turn_runner: Arc<Mutex<Arc<dyn TurnRunner>>>,
    pub(crate) model_executor: Arc<Mutex<Option<Arc<dyn LlmGateway>>>>,
    pub(crate) settings: Arc<Mutex<HostSettings>>,
    pub(crate) model_registry: Arc<Mutex<ModelRegistry>>,
    pub(crate) project_settings_path: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for HostApp {
    fn default() -> Self {
        Self::new()
    }
}

impl HostApp {
    fn default_turn_runner() -> Arc<dyn TurnRunner> {
        Arc::new(ErrorTurnRunner::new("turn runner not configured"))
    }

    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: None,
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(Self::default_turn_runner())),
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
        Self::with_storage_and_runner(storage, Self::default_turn_runner())
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
}
