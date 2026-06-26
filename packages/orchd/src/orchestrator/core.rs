// ---- Orchestrator: core — OrchCore struct + trait implementations ----
//
// OrchCore is the central runtime struct. It holds all orchestrator state
// and implements both the internal `Orchestrator` trait (used by
// TaskControlProvider) and the public `OrchRuntime` trait (used by Host).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{RwLock, oneshot};

use crate::actors::agent::types::{AgentTaskResultExt, ModelConfig};
use crate::model::executor::ModelStepExecutor;
use crate::protocol::agents::{AgentSpec, AgentTask, AgentTaskId};
use crate::protocol::config::{OrchdConfig, OrchdError, TaskInput, TaskResult, UserResponse};
use crate::protocol::event_store::OrchSourcingEvent;
use crate::protocol::events::{HostEvent, OrchEvent};
use crate::protocol::orch_runtime::OrchRuntime;
use crate::protocol::runtime::{
    GraphSnapshot, OrchModelConfig, OrchRunOptions, OrchRunResult, OrchestratorRuntimeConfig,
};
use crate::protocol::state::OrchState;
use crate::protocol::tools::{ToolProvider, ToolSet};
use crate::tools::registry::ToolRegistryImpl;
use crate::tools::task_control_provider::TaskControlProvider;

use super::agent::{register_agent, unregister_agent};
use super::state::{get_graph, snapshot, subscribe, update_plan};
use super::task::{await_task, cancel_task, run, spawn, spawn_detached};
use super::tool::{register_provider, register_tool_set, set_model_config, unregister_tool_set};

// ---- HostEventListener type alias ----

/// Listener for internal host events.
pub type HostEventListenerFn = Box<dyn Fn(HostEvent) + Send + Sync>;

// ---- OrchCore struct ----

/// Central orchestrator runtime.
///
/// Holds all state: tool registry, agent specs, model executor,
/// event listeners, sourcing journal, and pending detached task receivers.
pub struct OrchCore {
    pub run_id: String,
    pub tool_registry: Arc<ToolRegistryImpl>,
    pub model_executor: Arc<dyn ModelStepExecutor>,
    pub(crate) sourcing_events: RwLock<Vec<OrchSourcingEvent>>,
    pub(crate) latest_model_config: Arc<RwLock<Option<ModelConfig>>>,
    pub default_agent_id: String,
    pub(crate) agent_specs: Arc<RwLock<HashMap<String, AgentSpec>>>,
    pub(crate) allocated_task_ids: Arc<RwLock<HashSet<String>>>,
    pub(crate) _max_concurrent_agents: usize,
    pub(crate) listeners: Arc<RwLock<HashMap<u64, Arc<dyn Fn(serde_json::Value) + Send + Sync>>>>,
    pub(crate) next_listener_id: std::sync::atomic::AtomicU64,
    /// Pending oneshot receivers for detached tasks, keyed by task_id.
    pub(crate) pending_detached:
        Arc<tokio::sync::Mutex<HashMap<String, oneshot::Receiver<AgentTaskResultExt>>>>,
}

impl OrchCore {
    // ---- Internal constructors (pub(crate)) ----

    /// Internal constructor. Prefer `from_config()` for external use.
    pub(crate) fn new(
        model_executor: Arc<dyn ModelStepExecutor>,
        config: Option<OrchModelConfig>,
        runtime_config: Option<OrchestratorRuntimeConfig>,
    ) -> Self {
        let run_id = format!(
            "run_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let max_concurrent = runtime_config
            .and_then(|c| c.max_concurrent_agents)
            .map(|n| n as usize)
            .unwrap_or(usize::MAX);

        let listeners: Arc<RwLock<HashMap<u64, Arc<dyn Fn(serde_json::Value) + Send + Sync>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Build emit function for ToolRegistryImpl
        let listeners_for_registry = Arc::clone(&listeners);
        let emit_for_registry: std::sync::Arc<
            dyn Fn(HostEvent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
        > = Arc::new(move |event: HostEvent| {
            let listeners = Arc::clone(&listeners_for_registry);
            Box::pin(async move {
                let val = serde_json::to_value(&event).unwrap_or_default();
                let ls = listeners.read().await;
                for listener in ls.values() {
                    listener(val.clone());
                }
            })
        });

        let tool_registry = Arc::new(ToolRegistryImpl::new(emit_for_registry));

        let model_config = config.map(|c| ModelConfig {
            model: crate::model::types::ModelSpec {
                id: c.model.id.clone(),
                name: c.model.name.clone(),
                provider: c.model.provider.clone(),
            },
            provider: c.provider,
            settings: c.settings,
        });

        Self {
            run_id,
            tool_registry,
            model_executor,
            sourcing_events: RwLock::new(Vec::new()),
            latest_model_config: Arc::new(RwLock::new(model_config)),
            default_agent_id: "main".into(),
            agent_specs: Arc::new(RwLock::new(HashMap::new())),
            allocated_task_ids: Arc::new(RwLock::new(HashSet::new())),
            _max_concurrent_agents: max_concurrent,
            listeners,
            next_listener_id: std::sync::atomic::AtomicU64::new(0),
            pending_detached: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Internal init: register built-in tool providers.
    pub(crate) async fn init(self: &Arc<Self>) {
        let orch_provider = TaskControlProvider::new();
        orch_provider.set_orchestrator(self.clone());
        self.tool_registry
            .register_provider(Box::new(orch_provider))
            .await;

        // Load policy from .piko/sandbox.json if exists, otherwise fall back to permissive
        let policy_path = std::path::Path::new(".piko/sandbox.json");
        let policy = if policy_path.exists() {
            match piko_sandbox::policy::Policy::load(policy_path) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        "Failed to load sandbox policy from {}: {}, using permissive",
                        policy_path.display(),
                        e
                    );
                    piko_sandbox::policy::Policy {
                        version: 1,
                        read: vec![std::path::PathBuf::from(".")],
                        write: vec![std::path::PathBuf::from(".")],
                        deny: vec![std::path::PathBuf::from(".git")],
                        allowed_commands: default_allowed_commands(),
                        allow_network: false,
                    }
                }
            }
        } else {
            piko_sandbox::policy::Policy {
                version: 1,
                read: vec![std::path::PathBuf::from(".")],
                write: vec![std::path::PathBuf::from(".")],
                deny: vec![std::path::PathBuf::from(".git")],
                allowed_commands: default_allowed_commands(),
                allow_network: false,
            }
        };

        let workspace_provider = crate::tools::WorkspaceToolProvider::new(policy);
        self.tool_registry
            .register_provider(Box::new(workspace_provider))
            .await;
    }

    // ---- Public constructor ----

    /// Create an OrchCore from an OrchdConfig.
    ///
    /// Wires providers, registers agents, and initializes built-in tools
    /// in one call. This is the recommended entry point.
    pub async fn from_config(
        model_executor: Arc<dyn ModelStepExecutor>,
        config: OrchdConfig,
    ) -> Arc<Self> {
        let runtime_config = Some(OrchestratorRuntimeConfig {
            max_concurrent_agents: config.runtime.max_concurrent_agents,
        });

        let model_config = {
            let model = crate::protocol::messages::Model {
                id: config.default_model.model_id.clone(),
                name: config.default_model.model_id.clone(),
                provider: config.default_model.provider.clone(),
                base_url: None,
            };
            let provider = config
                .providers
                .get(&config.default_model.provider)
                .map(|p| crate::protocol::model::ModelProviderConfig {
                    api_key: Some(p.api_key.clone()),
                    base_url: p.base_url.clone(),
                    headers: Some(p.headers.clone().unwrap_or_default()),
                    reasoning: None,
                    session_id: None,
                    extra: None,
                })
                .unwrap_or_default();

            Some(OrchModelConfig {
                model,
                provider,
                settings: config.default_settings.clone(),
            })
        };

        let core = Self::new(model_executor, model_config, runtime_config);
        let arc = Arc::new(core);

        // Auto-register agents from config
        for spec in config.agents.values() {
            arc.register_agent(spec.clone()).await;
        }

        // Initialize built-in tool providers
        arc.init().await;

        arc
    }
}

// ---- Inherent orchestrator methods (used by TaskControlProvider and RPC) ----

impl OrchCore {
    // ── Event sourcing helper ──

    /// Append a sourcing event to the journal.
    async fn emit_sourcing(&self, event: OrchSourcingEvent) {
        self.sourcing_events.write().await.push(event);
    }

    /// Return all sourcing events (for tests/debugging).
    pub async fn sourcing_events(&self) -> Vec<OrchSourcingEvent> {
        self.sourcing_events.read().await.clone()
    }

    // ── Agent methods ──

    pub async fn register_agent(&self, spec: AgentSpec) {
        let agent_id = spec.id.clone();
        register_agent(self, spec.clone()).await;
        self.emit_sourcing(OrchSourcingEvent::AgentRegistered {
            agent_id,
            spec,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    pub async fn unregister_agent(&self, agent_id: &str) {
        unregister_agent(self, agent_id.to_string()).await;
        self.emit_sourcing(OrchSourcingEvent::AgentUnregistered {
            agent_id: agent_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    // ── Tool set methods ──

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        register_tool_set(self, tool_set.clone()).await;
        self.emit_sourcing(OrchSourcingEvent::ToolSetRegistered {
            tool_set,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    pub async fn unregister_tool_set(&self, tool_set_id: &str) {
        unregister_tool_set(self, tool_set_id.to_string()).await;
        self.emit_sourcing(OrchSourcingEvent::ToolSetUnregistered {
            tool_set_id: tool_set_id.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    // ── Model config ──

    pub async fn set_model_config(&self, config: OrchModelConfig) {
        set_model_config(self, config.clone()).await;
        self.emit_sourcing(OrchSourcingEvent::ModelConfigSet {
            model_id: config.model.id.clone(),
            provider_name: config.model.provider.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    // ── Provider ──

    pub async fn register_provider(&self, provider: Box<dyn ToolProvider>) {
        register_provider(self, provider).await
    }

    pub async fn unregister_provider(&self, provider_id: &str) {
        self.tool_registry.unregister_provider(provider_id).await;
    }

    // ── Task methods ──

    pub async fn spawn(&self, mut task: AgentTask) -> AgentTaskId {
        let task_id = task.id.clone().unwrap_or_else(|| {
            format!(
                "task_{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .chars()
                    .take(12)
                    .collect::<String>()
            )
        });
        task.id = Some(task_id.clone());
        let target_agent_id = task.target_agent_id.clone();
        let prompt = task.prompt.clone();
        let source = task.source.clone();
        let parent_task_id = task.parent_task_id.clone();

        // Emit task created event
        self.emit_sourcing(OrchSourcingEvent::TaskCreated {
            task_id: task_id.clone(),
            target_agent_id: target_agent_id.clone(),
            prompt: prompt.clone(),
            source: source.clone(),
            parent_task_id: parent_task_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;

        let actual_task_id = spawn(self, task).await;

        // Emit task started
        self.emit_sourcing(OrchSourcingEvent::TaskStarted {
            task_id: actual_task_id.clone(),
            agent_id: target_agent_id.clone(),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;

        actual_task_id
    }

    pub async fn spawn_detached(&self, mut task: AgentTask) -> AgentTaskId {
        let task_id = task.id.clone().unwrap_or_else(|| {
            format!(
                "task_{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .chars()
                    .take(12)
                    .collect::<String>()
            )
        });
        task.id = Some(task_id.clone());
        let target_agent_id = task.target_agent_id.clone();
        let prompt = task.prompt.clone();
        let source = task.source.clone();
        let parent_task_id = task.parent_task_id.clone();

        let actual_task_id = spawn_detached(self, task).await;

        self.emit_sourcing(OrchSourcingEvent::TaskCreated {
            task_id: actual_task_id.clone(),
            target_agent_id,
            prompt,
            source,
            parent_task_id,
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;

        actual_task_id
    }

    pub async fn await_task(&self, task_id: &str) -> Option<serde_json::Value> {
        await_task(self, task_id.to_string()).await
    }

    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        run(self, prompt.to_string(), opts).await
    }

    pub async fn cancel_task(&self, task_id: &str, reason: Option<&str>) {
        cancel_task(self, task_id.to_string(), reason.map(|r| r.to_string())).await;

        self.emit_sourcing(OrchSourcingEvent::TaskCancelled {
            task_id: task_id.to_string(),
            agent_id: String::new(),
            reason: reason.map(|r| r.to_string()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        })
        .await;
    }

    pub async fn set_approval_gateway(
        &self,
        gateway: Option<Box<dyn crate::protocol::approval::ApprovalGateway>>,
    ) {
        crate::orchestrator::tool::set_approval_gateway(self, gateway).await;
    }

    pub async fn subscribe(
        &self,
        listener: Box<dyn Fn(HostEvent) + Send + Sync>,
    ) -> Box<dyn FnOnce() + Send> {
        subscribe(self, listener).await
    }

    pub async fn snapshot(&self) -> OrchState {
        snapshot(self).await
    }

    pub async fn update_plan(&self, agent_id: &str, task_id: &str, plan: Vec<serde_json::Value>) {
        update_plan(self, agent_id.to_string(), task_id.to_string(), plan).await
    }

    pub async fn get_graph(&self) -> GraphSnapshot {
        get_graph(self).await
    }
}

// ---- Public OrchRuntime trait implementation (Host-facing API) ----

impl OrchRuntime for OrchCore {
    fn configure(
        &self,
        config: OrchdConfig,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        let agents = config.agents.clone();
        let default_model = config.default_model.clone();
        let default_settings = config.default_settings.clone();
        let providers = config.providers.clone();

        Box::pin(async move {
            // Register agents
            for spec in agents.values() {
                self.register_agent(spec.clone()).await;
            }

            // Build model config
            let model = crate::protocol::messages::Model {
                id: default_model.model_id.clone(),
                name: default_model.model_id.clone(),
                provider: default_model.provider.clone(),
                base_url: None,
            };
            let provider = providers
                .get(&default_model.provider)
                .map(|p| crate::protocol::model::ModelProviderConfig {
                    api_key: Some(p.api_key.clone()),
                    base_url: p.base_url.clone(),
                    headers: Some(p.headers.clone().unwrap_or_default()),
                    reasoning: None,
                    session_id: None,
                    extra: None,
                })
                .unwrap_or_default();

            self.set_model_config(OrchModelConfig {
                model,
                provider,
                settings: default_settings,
            })
            .await;

            Ok(())
        })
    }

    fn shutdown(&self) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        Box::pin(async move { Ok(()) })
    }

    fn register_agent(
        &self,
        spec: AgentSpec,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        Box::pin(async move {
            self.register_agent(spec).await;
            Ok(())
        })
    }

    fn unregister_agent(
        &self,
        agent_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        let agent_id = agent_id.to_string();
        Box::pin(async move {
            self.unregister_agent(&agent_id).await;
            Ok(())
        })
    }

    fn run(
        &self,
        input: TaskInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskResult, OrchdError>> + Send + '_>> {
        Box::pin(async move {
            let task = input.convert_to_agent_task(crate::protocol::agents::TaskSource::User);
            let task_id = self.spawn_detached(task).await;
            let result = self.await_task(&task_id).await;

            match result {
                Some(val) => {
                    let messages = val
                        .get("messages")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let total_steps =
                        val.get("total_steps").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let status_str = val
                        .get("final_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("completed");
                    let status = match status_str {
                        "completed" => crate::protocol::config::TaskStatus::Completed,
                        "failed" => crate::protocol::config::TaskStatus::Error,
                        "cancelled" => crate::protocol::config::TaskStatus::Aborted,
                        _ => crate::protocol::config::TaskStatus::Completed,
                    };
                    Ok(TaskResult {
                        task_id,
                        status,
                        messages,
                        total_steps,
                        usage: None,
                    })
                }
                None => Err(OrchdError::internal("Task result not available")),
            }
        })
    }

    fn spawn(
        &self,
        input: TaskInput,
    ) -> Pin<Box<dyn Future<Output = Result<AgentTaskId, OrchdError>> + Send + '_>> {
        Box::pin(async move {
            let task = input.convert_to_agent_task(crate::protocol::agents::TaskSource::User);
            let task_id = self.spawn_detached(task).await;
            Ok(task_id)
        })
    }

    fn join(
        &self,
        task_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<TaskResult, OrchdError>> + Send + '_>> {
        let task_id = task_id.to_string();
        Box::pin(async move {
            match self.await_task(&task_id).await {
                Some(val) => {
                    let messages = val
                        .get("messages")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let total_steps =
                        val.get("total_steps").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let status_str = val
                        .get("final_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("completed");
                    let status = match status_str {
                        "completed" => crate::protocol::config::TaskStatus::Completed,
                        "failed" => crate::protocol::config::TaskStatus::Error,
                        "cancelled" => crate::protocol::config::TaskStatus::Aborted,
                        _ => crate::protocol::config::TaskStatus::Completed,
                    };
                    Ok(TaskResult {
                        task_id,
                        status,
                        messages,
                        total_steps,
                        usage: None,
                    })
                }
                None => Err(OrchdError::not_found(format!("Task {task_id} not found"))),
            }
        })
    }

    fn cancel(
        &self,
        task_id: &str,
        reason: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        let task_id = task_id.to_string();
        let reason = reason.to_string();
        Box::pin(async move {
            crate::orchestrator::task::cancel_task(self, task_id, Some(reason)).await;
            Ok(())
        })
    }

    fn respond_user(
        &self,
        _task_id: &str,
        _response: UserResponse,
    ) -> Pin<Box<dyn Future<Output = Result<(), OrchdError>> + Send + '_>> {
        Box::pin(async move {
            // TODO: wire to UserInteractionProvider callback
            Ok(())
        })
    }

    fn subscribe(
        &self,
        listener: Box<dyn Fn(OrchEvent) + Send + Sync>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn FnOnce() + Send>, OrchdError>> + Send + '_>>
    {
        Box::pin(async move {
            let host_listener: Box<dyn Fn(HostEvent) + Send + Sync> = Box::new(move |host_ev| {
                if let Ok(orch_ev) = OrchEvent::try_from(host_ev) {
                    listener(orch_ev);
                }
            });
            let cleanup = self.subscribe(host_listener).await;
            Ok(cleanup)
        })
    }

    fn snapshot(&self) -> Pin<Box<dyn Future<Output = Result<OrchState, OrchdError>> + Send + '_>> {
        Box::pin(async move { Ok(self.snapshot().await) })
    }

    fn graph(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<GraphSnapshot, OrchdError>> + Send + '_>> {
        Box::pin(async move { Ok(self.get_graph().await) })
    }
}

fn default_allowed_commands() -> Vec<String> {
    vec![
        "ls".into(),
        "cat".into(),
        "head".into(),
        "tail".into(),
        "find".into(),
        "grep".into(),
        "rg".into(),
        "git".into(),
        "echo".into(),
        "mkdir".into(),
        "cp".into(),
        "mv".into(),
        "rm".into(),
        "wc".into(),
        "sort".into(),
        "uniq".into(),
        "sed".into(),
        "awk".into(),
        "diff".into(),
        "npm".into(),
        "npx".into(),
        "node".into(),
        "bun".into(),
        "cargo".into(),
        "python3".into(),
        "python".into(),
        "go".into(),
        "make".into(),
        "rustc".into(),
        "tsc".into(),
        "biome".into(),
        "prettier".into(),
    ]
}
