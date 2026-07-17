use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_llmd::auth::AuthStorage;
use piko_llmd::gateway::LlmGateway;
use tokio::sync::Mutex;

use crate::domain::config::{HostSettings, ModelRegistry};
use crate::domain::sessions::HostState;
use crate::ports::prompt_materials::PromptMaterialLoader;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;
use crate::ports::{AgentRunRunner, ErrorAgentRunRunner};

/// Application-layer composition root.
///
/// Owns all mutable host state plus the ports (turn runner, model executor,
/// storage) that use-cases in `application::{turns, sessions, compaction}`
/// orchestrate. `protocol::HostServer` is a thin newtype wrapper around this
/// type: command routing/transport lives in `protocol`, use-case bodies live
/// here.
///
/// `application::turns` / `application::sessions` bodies only ever see the
/// port traits (`SessionRepositoryPort`, `SessionStoreFactory`,
/// `PromptMaterialLoader`, `AgentRunRunner`) — never `crate::infra` or
/// `crate::adapters` directly. The default-adapter wiring below is the one
/// sanctioned exception: as the composition root, `HostApp`'s constructors
/// build the real filesystem-backed adapters so `HostServer::new()` and the
/// legacy `with_*` constructors keep working without a caller-supplied
/// factory.
#[derive(Clone)]
pub struct HostApp {
    pub(crate) state: Arc<Mutex<HostState>>,
    pub(crate) storage: Option<Arc<dyn SessionRepositoryPort>>,
    pub(crate) session_paths: Arc<Mutex<HashMap<String, PathBuf>>>,
    pub(crate) turn_runner: Arc<Mutex<Arc<dyn AgentRunRunner>>>,
    pub(crate) model_executor: Arc<Mutex<Option<Arc<dyn LlmGateway>>>>,
    pub(crate) settings: Arc<Mutex<HostSettings>>,
    pub(crate) model_registry: Arc<Mutex<ModelRegistry>>,
    pub(crate) project_settings_path: Arc<Mutex<Option<PathBuf>>>,
    pub(crate) session_store_factory: Arc<dyn SessionStoreFactory>,
    pub(crate) prompt_materials: Arc<dyn PromptMaterialLoader>,
}

impl Default for HostApp {
    fn default() -> Self {
        Self::new()
    }
}

impl HostApp {
    fn default_turn_runner() -> Arc<dyn AgentRunRunner> {
        Arc::new(ErrorAgentRunRunner::new("turn runner not configured"))
    }

    /// Default filesystem-backed session store factory (see composition-root
    /// note on [`HostApp`]).
    fn default_session_store_factory() -> Arc<dyn SessionStoreFactory> {
        Arc::new(crate::adapters::storage::FsSessionStoreFactory)
    }

    /// Default filesystem-backed prompt material loader (see composition-root
    /// note on [`HostApp`]).
    fn default_prompt_materials() -> Arc<dyn PromptMaterialLoader> {
        Arc::new(crate::adapters::prompts::FsPromptMaterialLoader)
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
            session_store_factory: Self::default_session_store_factory(),
            prompt_materials: Self::default_prompt_materials(),
        }
    }

    pub fn with_storage(storage: impl SessionRepositoryPort + 'static) -> Self {
        Self::with_storage_and_runner(storage, Self::default_turn_runner())
    }

    pub fn with_turn_runner(turn_runner: Arc<dyn AgentRunRunner>) -> Self {
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
            session_store_factory: Self::default_session_store_factory(),
            prompt_materials: Self::default_prompt_materials(),
        }
    }

    pub fn with_storage_and_runner(
        storage: impl SessionRepositoryPort + 'static,
        turn_runner: Arc<dyn AgentRunRunner>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: Some(Arc::new(storage)),
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(HostSettings::default())),
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(
                AuthStorage::in_memory(std::collections::HashMap::new()),
                vec![],
            ))),
            project_settings_path: Arc::new(Mutex::new(None)),
            session_store_factory: Self::default_session_store_factory(),
            prompt_materials: Self::default_prompt_materials(),
        }
    }

    pub fn with_storage_runner_settings(
        storage: impl SessionRepositoryPort + 'static,
        turn_runner: Arc<dyn AgentRunRunner>,
        settings: HostSettings,
    ) -> Self {
        let auth = AuthStorage::create(None)
            .unwrap_or_else(|_| AuthStorage::in_memory(std::collections::HashMap::new()));
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: Some(Arc::new(storage)),
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            turn_runner: Arc::new(Mutex::new(turn_runner)),
            model_executor: Arc::new(Mutex::new(None)),
            settings: Arc::new(Mutex::new(settings)),
            model_registry: Arc::new(Mutex::new(ModelRegistry::new(auth, vec![]))),
            project_settings_path: Arc::new(Mutex::new(None)),
            session_store_factory: Self::default_session_store_factory(),
            prompt_materials: Self::default_prompt_materials(),
        }
    }

    /// Set the model executor (used for compaction and other host-level LLM calls).
    pub async fn set_model_executor(&self, executor: Arc<dyn LlmGateway>) {
        *self.model_executor.lock().await = Some(executor);
    }
}
