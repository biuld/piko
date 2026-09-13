use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use piko_llmd::auth::AuthStorage;
use piko_llmd::gateway::InferenceGateway;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::domain::config::{HostSettings, ModelRegistry};
use crate::domain::sessions::{HostState, SessionModelRef};
use crate::ports::prompt_materials::PromptMaterialLoader;
use crate::ports::session_repository::SessionRepositoryPort;
use crate::ports::session_store::SessionStoreFactory;
use crate::ports::{AgentRunRunner, AgentWorkAddress, ErrorAgentRunRunner, TranscriptEstimator};

/// Application-layer composition root.
///
/// Owns all mutable host state plus the ports (turn runner, model executor,
/// storage) that use-cases in `application::{turns, sessions, compaction}`
/// orchestrate. `protocol::HostServer` is a thin newtype wrapper around this
/// type: command routing/transport lives in `protocol`, use-case bodies live
/// here.
///
/// `application::agent_work` / `application::sessions` bodies only ever see the
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
    /// The live agent runner bundle, swapped atomically by
    /// [`HostApp::swap_agent_runtime_bundle`].
    pub(crate) runner_bundle: Arc<Mutex<AgentRuntimeBundle>>,
    /// Runner generation bound to each admitted work item. Old runners stay
    /// reachable through this map until their last work item reaches the
    /// observation barrier, so approval/interaction/control requests cannot
    /// be redirected to a replacement generation.
    pub(crate) work_runners: Arc<Mutex<HashMap<AgentWorkAddress, BoundWorkRunner>>>,
    pub(crate) settings: Arc<Mutex<HostSettings>>,
    pub(crate) model_registry: Arc<Mutex<ModelRegistry>>,
    pub(crate) auth_logins: Arc<Mutex<HashMap<String, ActiveAuthLogin>>>,
    pub(crate) project_settings_path: Arc<Mutex<Option<PathBuf>>>,
    pub(crate) session_store_factory: Arc<dyn SessionStoreFactory>,
    pub(crate) prompt_materials: Arc<dyn PromptMaterialLoader>,
    pub(crate) transcript_estimator: Arc<dyn TranscriptEstimator>,
}

/// The runtime surfaces an admitted turn binds to. `generation` increments on
/// every hot swap (config change, auth login/logout); control requests
/// (approval, interrupt, steer, cancel, terminal trajectory) must resolve
/// against the runner generation the work started on through
/// `work_runners`.
#[derive(Clone)]
pub(crate) struct AgentRuntimeBundle {
    pub(crate) runner: Arc<dyn AgentRunRunner>,
    pub(crate) model_executor: Option<Arc<dyn InferenceGateway>>,
    /// The resolved provider+model the current turn runner executes with.
    /// This is the single source of truth for session model continuity:
    /// turn submission records it per session (durable), and the prompt
    /// model-switch fragment and JSONL `ModelChange` marker derive from the
    /// session record.
    pub(crate) active_model: Option<SessionModelRef>,
    pub(crate) generation: u64,
}

#[derive(Clone)]
pub(crate) struct BoundWorkRunner {
    pub(crate) runner: Arc<dyn AgentRunRunner>,
    pub(crate) generation: u64,
}

#[derive(Clone)]
pub(crate) struct ActiveAuthLogin {
    pub(crate) login_id: String,
    pub(crate) cancellation: CancellationToken,
}

impl Default for HostApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependencies for constructing a [`HostApp`]. Fields left as `None` get
/// composition-root defaults so the legacy `with_*` constructors keep
/// working without a caller-supplied factory.
pub(crate) struct HostDependencies {
    pub(crate) storage: Option<Arc<dyn SessionRepositoryPort>>,
    pub(crate) agent_runner: Option<Arc<dyn AgentRunRunner>>,
    pub(crate) settings: HostSettings,
    pub(crate) model_registry: Option<ModelRegistry>,
    pub(crate) session_store_factory: Option<Arc<dyn SessionStoreFactory>>,
    pub(crate) prompt_materials: Option<Arc<dyn PromptMaterialLoader>>,
    pub(crate) transcript_estimator: Option<Arc<dyn TranscriptEstimator>>,
}

impl HostDependencies {
    fn with_storage_and_runner(
        storage: Option<Arc<dyn SessionRepositoryPort>>,
        agent_runner: Arc<dyn AgentRunRunner>,
    ) -> Self {
        Self {
            storage,
            agent_runner: Some(agent_runner),
            settings: HostSettings::default(),
            model_registry: None,
            session_store_factory: None,
            prompt_materials: None,
            transcript_estimator: None,
        }
    }
}

impl HostApp {
    fn default_agent_runner() -> Arc<dyn AgentRunRunner> {
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

    fn default_transcript_estimator() -> Arc<dyn TranscriptEstimator> {
        Arc::new(crate::adapters::bookkeeping::OrchTranscriptEstimator)
    }

    fn build(deps: HostDependencies) -> Self {
        let runner = deps.agent_runner.unwrap_or_else(Self::default_agent_runner);
        Self {
            state: Arc::new(Mutex::new(HostState::new())),
            storage: deps.storage,
            session_paths: Arc::new(Mutex::new(HashMap::new())),
            runner_bundle: Arc::new(Mutex::new(AgentRuntimeBundle {
                runner,
                model_executor: None,
                active_model: None,
                generation: 0,
            })),
            work_runners: Arc::new(Mutex::new(HashMap::new())),
            settings: Arc::new(Mutex::new(deps.settings)),
            model_registry: Arc::new(Mutex::new(deps.model_registry.unwrap_or_else(|| {
                ModelRegistry::new(
                    AuthStorage::in_memory(std::collections::HashMap::new()),
                    vec![],
                )
            }))),
            auth_logins: Arc::new(Mutex::new(HashMap::new())),
            project_settings_path: Arc::new(Mutex::new(None)),
            session_store_factory: deps
                .session_store_factory
                .unwrap_or_else(Self::default_session_store_factory),
            prompt_materials: deps
                .prompt_materials
                .unwrap_or_else(Self::default_prompt_materials),
            transcript_estimator: deps
                .transcript_estimator
                .unwrap_or_else(Self::default_transcript_estimator),
        }
    }

    pub fn new() -> Self {
        Self::build(HostDependencies {
            storage: None,
            agent_runner: None,
            settings: HostSettings::default(),
            model_registry: None,
            session_store_factory: None,
            prompt_materials: None,
            transcript_estimator: None,
        })
    }

    pub fn with_storage(storage: impl SessionRepositoryPort + 'static) -> Self {
        Self::with_storage_and_runner(storage, Self::default_agent_runner())
    }

    pub fn with_agent_runner(agent_runner: Arc<dyn AgentRunRunner>) -> Self {
        Self::build(HostDependencies::with_storage_and_runner(
            None,
            agent_runner,
        ))
    }

    pub fn with_storage_and_runner(
        storage: impl SessionRepositoryPort + 'static,
        agent_runner: Arc<dyn AgentRunRunner>,
    ) -> Self {
        Self::build(HostDependencies::with_storage_and_runner(
            Some(Arc::new(storage)),
            agent_runner,
        ))
    }

    pub fn with_storage_runner_settings(
        storage: impl SessionRepositoryPort + 'static,
        agent_runner: Arc<dyn AgentRunRunner>,
        settings: HostSettings,
    ) -> Self {
        let auth = AuthStorage::create(None)
            .unwrap_or_else(|_| AuthStorage::in_memory(std::collections::HashMap::new()));
        Self::build(HostDependencies {
            storage: Some(Arc::new(storage)),
            agent_runner: Some(agent_runner),
            settings,
            model_registry: Some(ModelRegistry::new(auth, vec![])),
            session_store_factory: None,
            prompt_materials: None,
            transcript_estimator: None,
        })
    }

    /// Snapshot the live runner bundle. Admitted work captures this once and
    /// keeps using the captured runner for the whole turn lifecycle.
    pub(crate) async fn current_runner(&self) -> Arc<dyn AgentRunRunner> {
        self.runner_bundle.lock().await.runner.clone()
    }

    /// Snapshot the live runner bundle with its generation tag. Admitted work
    /// captures this once at submit time and keeps using the captured runner
    /// for the whole turn lifecycle, so a hot swap never re-routes an
    /// in-flight turn's control plane to the replacement runner (P1-1).
    pub(crate) async fn current_runtime_bundle(&self) -> AgentRuntimeBundle {
        self.runner_bundle.lock().await.clone()
    }

    pub(crate) async fn bind_work_runner(
        &self,
        address: AgentWorkAddress,
        runner: Arc<dyn AgentRunRunner>,
        generation: u64,
    ) {
        self.work_runners
            .lock()
            .await
            .insert(address, BoundWorkRunner { runner, generation });
    }

    pub(crate) async fn release_work_runner(&self, address: &AgentWorkAddress) {
        self.work_runners.lock().await.remove(address);
    }

    pub(crate) async fn bound_runner_for_work(
        &self,
        address: &AgentWorkAddress,
    ) -> Option<Arc<dyn AgentRunRunner>> {
        self.work_runners
            .lock()
            .await
            .get(address)
            .map(|binding| binding.runner.clone())
    }

    pub(crate) async fn bound_runner_for_agent(
        &self,
        session_id: &str,
        agent_instance_id: &str,
    ) -> Option<Arc<dyn AgentRunRunner>> {
        self.work_runners
            .lock()
            .await
            .iter()
            .filter(|(address, _)| {
                address.session_id == session_id && address.agent_instance_id == agent_instance_id
            })
            .max_by_key(|(_, binding)| binding.generation)
            .map(|(_, binding)| binding.runner.clone())
    }

    /// Candidate control planes for a session, newest first. Pointer
    /// deduplication keeps multiple work roots on one generation from causing
    /// duplicate control requests.
    pub(crate) async fn session_runner_candidates(
        &self,
        session_id: &str,
    ) -> Vec<Arc<dyn AgentRunRunner>> {
        let current = self.current_runner().await;
        let bindings = self.work_runners.lock().await;
        let mut bound = bindings
            .iter()
            .filter(|(address, _)| address.session_id == session_id)
            .map(|(_, binding)| (binding.generation, binding.runner.clone()))
            .collect::<Vec<_>>();
        bound.sort_by_key(|item| std::cmp::Reverse(item.0));
        let mut runners = Vec::new();
        for (_, runner) in bound {
            if !runners.iter().any(|known| Arc::ptr_eq(known, &runner)) {
                runners.push(runner);
            }
        }
        if !runners.iter().any(|known| Arc::ptr_eq(known, &current)) {
            runners.push(current);
        }
        runners
    }

    pub(crate) async fn runner_for_session_control(
        &self,
        session_id: &str,
    ) -> Arc<dyn AgentRunRunner> {
        let candidates = self.session_runner_candidates(session_id).await;
        for runner in &candidates {
            if runner.has_active_session_run(session_id).await {
                return runner.clone();
            }
            let (approvals, interactions) = runner.pending_prompts_for_session(session_id).await;
            if !approvals.is_empty() || !interactions.is_empty() {
                return runner.clone();
            }
        }
        candidates
            .into_iter()
            .next()
            .unwrap_or_else(Self::default_agent_runner)
    }

    /// Atomically replace the whole runtime bundle {runner, executor,
    /// active_model}. The old executor is dropped even when the new one is
    /// `None` (e.g. invalid config after logout), so compaction and
    /// navigation summarization can never call a stale gateway (P2-5).
    ///
    /// The swap bumps `generation`; in-flight turns keep their captured
    /// runner until they finish. A work item outliving its generation
    /// degrades gracefully: control requests still land their durable
    /// journal facts, and the next runtime hydrates them.
    pub(crate) async fn swap_agent_runtime_bundle(
        &self,
        runner: Arc<dyn AgentRunRunner>,
        model_executor: Option<Arc<dyn InferenceGateway>>,
        active_model: Option<SessionModelRef>,
    ) {
        let mut bundle = self.runner_bundle.lock().await;
        *bundle = AgentRuntimeBundle {
            runner,
            model_executor,
            active_model,
            generation: bundle.generation + 1,
        };
    }

    /// The resolved provider+model the current turn runner executes with.
    /// Set the model executor (used for compaction and other host-level LLM calls).
    pub async fn set_model_executor(&self, executor: Arc<dyn InferenceGateway>) {
        self.runner_bundle.lock().await.model_executor = Some(executor);
    }

    /// Wire the `new_context_window` tool callback (F-05): a model-visible
    /// fresh-window request runs the host-owned token-budget compact. The
    /// rewrite has no per-command event sender, so the client refreshes on
    /// the next snapshot; the durable tree and the running execution are
    /// both trimmed immediately.
    pub(crate) async fn wire_context_window_callback(&self) {
        let runner = self.current_runner().await;
        self.wire_context_window_callback_for(&runner);
    }

    pub(crate) fn wire_context_window_callback_for(&self, runner: &Arc<dyn AgentRunRunner>) {
        let host = self.clone();
        let callback: piko_orchd::tools::NewContextWindowCallback =
            Arc::new(move |session_id, agent_instance_id| {
                let host = host.clone();
                Box::pin(async move {
                    host.compact_session_if_needed(
                        &session_id,
                        &agent_instance_id,
                        0,
                        piko_protocol::command::CompactMode::NewContextWindow,
                        true,
                        None,
                    )
                    .await
                })
            });
        runner.set_context_window_callback(callback);
    }

    /// Wire the F-11 guardian review callback: the approval gateway asks the
    /// application to run the bounded review over the durable session tree.
    pub(crate) async fn wire_guardian_callback(&self) {
        let runner = self.current_runner().await;
        self.wire_guardian_callback_for(&runner);
    }

    pub(crate) fn wire_guardian_callback_for(&self, runner: &Arc<dyn AgentRunRunner>) {
        let host = self.clone();
        let callback: crate::domain::guardian::GuardianReviewCallback =
            Arc::new(move |session_id, request| {
                let host = host.clone();
                Box::pin(async move { host.run_guardian_review(&session_id, &request).await })
            });
        runner.set_guardian_review_callback(callback);
    }

    /// Record the resolved provider+model the current turn runner executes
    /// with. This is the single source of truth for session model continuity:
    /// turn submission records it per session and drives the prompt
    /// model-switch fragment and the durable JSONL `ModelChange` marker.
    pub async fn set_active_model(&self, model: Option<SessionModelRef>) {
        self.runner_bundle.lock().await.active_model = model;
    }
}
