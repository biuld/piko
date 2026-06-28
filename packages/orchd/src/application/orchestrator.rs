// ---- Application: OrchCore — thin orchestrator facade ----
//
// OrchCore is the central orchestrator struct. It holds all orchestrator state
// and exposes methods for hostd to manage agents, tasks, and tool execution.
//
// This is a thin facade over domain entities, ports, and adapters.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelConfig;
use crate::domain::tasks::task::{AgentTask, AgentTaskId, AgentTaskState};
use crate::domain::tools::definition::ToolSet;
use crate::ports::model_gateway::LlmGateway;
use crate::ports::tool_provider::ToolProvider;
use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::adapters::tools::task_control_provider::TaskControlProvider;
use piko_protocol::config::{OrchdConfig, SandboxConfig};
use piko_protocol::runtime::{
    GraphSnapshot, OrchModelConfig, OrchRunOptions, OrchRunResult, OrchestratorRuntimeConfig,
};
use piko_protocol::state::OrchState;

use super::agents::{register_agent, unregister_agent};
use super::snapshots::{get_graph, snapshot, update_plan};
use super::tasks::{
    PendingDetachedTask, await_task, cancel_task, run, spawn, spawn_detached, steer_task,
};
use super::tools::{register_provider, register_tool_set, set_model_config, unregister_tool_set};

// ---- OrchCore struct ----

/// Central orchestrator runtime.
///
/// Holds runtime state: tool registry, agent specs, model executor,
/// event listeners, task projections, and pending detached task receivers.
pub struct OrchCore {
    pub run_id: String,
    pub tool_registry: Arc<ToolRegistryImpl>,
    pub model_executor: Arc<dyn LlmGateway>,
    pub(crate) latest_model_config: Arc<RwLock<Option<ModelConfig>>>,
    pub default_agent_id: String,
    pub(crate) agent_specs: Arc<RwLock<HashMap<String, AgentSpec>>>,
    pub(crate) task_states: Arc<RwLock<HashMap<String, AgentTaskState>>>,
    pub(crate) allocated_task_ids: Arc<RwLock<HashSet<String>>>,
    pub(crate) _max_concurrent_agents: usize,
    pub(crate) listeners: Arc<RwLock<HashMap<u64, Arc<dyn Fn(serde_json::Value) + Send + Sync>>>>,
    pub(crate) next_listener_id: std::sync::atomic::AtomicU64,
    /// Pending oneshot receivers for detached tasks, keyed by task_id.
    pub(crate) pending_detached: Arc<tokio::sync::Mutex<HashMap<String, PendingDetachedTask>>>,
}

impl OrchCore {
    // ---- Internal constructors (pub(crate)) ----

    /// Internal constructor. Prefer `from_config()` for external use.
    pub(crate) fn new(
        model_executor: Arc<dyn LlmGateway>,
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
            dyn Fn(Event) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
        > = Arc::new(move |event: Event| {
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
            model: crate::domain::model::step::ModelSpec {
                id: c.model.id.clone(),
                name: c.model.name.clone(),
                provider: c.model.provider.clone(),
            },
            provider: c.provider,
            settings: c.settings,
            thinking_level_map: c.thinking_level_map,
        });

        Self {
            run_id,
            tool_registry,
            model_executor,
            latest_model_config: Arc::new(RwLock::new(model_config)),
            default_agent_id: "main".into(),
            agent_specs: Arc::new(RwLock::new(HashMap::new())),
            task_states: Arc::new(RwLock::new(HashMap::new())),
            allocated_task_ids: Arc::new(RwLock::new(HashSet::new())),
            _max_concurrent_agents: max_concurrent,
            listeners,
            next_listener_id: std::sync::atomic::AtomicU64::new(0),
            pending_detached: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Internal init: register built-in tool providers.
    pub(crate) async fn init(self: &Arc<Self>, sandbox: &SandboxConfig) {
        let orch_provider = TaskControlProvider::new();
        orch_provider.set_orchestrator(self.clone()).await;
        self.tool_registry
            .register_provider(Box::new(orch_provider))
            .await;

        // Load sandbox policy from configured path, or use permissive defaults.
        let policy = if !sandbox.enabled {
            tracing::info!("Sandbox disabled, using permissive policy");
            permissive_policy()
        } else if let Some(ref policy_path) = sandbox.policy_path {
            let path = std::path::Path::new(policy_path);
            if path.exists() {
                match piko_sandbox::policy::Policy::load(path) {
                    Ok(p) => {
                        tracing::info!("Loaded sandbox policy from {}", path.display());
                        p
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load sandbox policy from {}: {}, using permissive",
                            path.display(),
                            e
                        );
                        permissive_policy()
                    }
                }
            } else {
                tracing::warn!(
                    "Sandbox policy path configured but file not found: {}, using permissive",
                    path.display()
                );
                permissive_policy()
            }
        } else {
            // sandbox.enabled is true but no policyPath → try default location
            let default_path = std::path::Path::new(".piko/sandbox.json");
            if default_path.exists() {
                match piko_sandbox::policy::Policy::load(default_path) {
                    Ok(p) => {
                        tracing::info!("Loaded sandbox policy from default location");
                        p
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load sandbox policy from default location: {}, using permissive",
                            e
                        );
                        permissive_policy()
                    }
                }
            } else {
                tracing::info!("No sandbox policy found, using permissive defaults");
                permissive_policy()
            }
        };

        let workspace_provider = if let Some(ref shell) = sandbox.shell_path {
            crate::adapters::tools::workspace_provider::WorkspaceToolProvider::with_shell(
                policy,
                shell.as_str(),
            )
        } else {
            crate::adapters::tools::workspace_provider::WorkspaceToolProvider::new(policy)
        };
        self.tool_registry
            .register_provider(Box::new(workspace_provider))
            .await;

        // Register default tool sets
        let builtin_toolset = ToolSet {
            id: "builtin".into(),
            name: "Built-in Tools".into(),
            description: Some("Built-in orchestrator tools".into()),
            metadata: None,
            policy: None,
            tools: vec![crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                provider_id: "orch".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        };
        self.tool_registry.register_tool_set(builtin_toolset).await;

        let workspace_toolset = ToolSet {
            id: "workspace".into(),
            name: "Workspace Tools".into(),
            description: Some("Local workspace tools".into()),
            metadata: None,
            policy: None,
            tools: vec![crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                provider_id: "workspace".into(),
                namespace: "".into(),
                alias: None,
                policy: None,
            }],
        };
        self.tool_registry
            .register_tool_set(workspace_toolset)
            .await;
    }

    // ---- Public constructor ----

    /// Create an OrchCore from an OrchdConfig.
    ///
    /// Wires providers, registers agents, and initializes built-in tools
    /// in one call. This is the recommended entry point.
    pub async fn from_config(
        model_executor: Arc<dyn LlmGateway>,
        config: OrchdConfig,
    ) -> Arc<Self> {
        let runtime_config = Some(OrchestratorRuntimeConfig {
            max_concurrent_agents: config.runtime.max_concurrent_agents,
        });

        let model_config = {
            let model = piko_protocol::messages::Model {
                id: config.default_model.model_id.clone(),
                name: config.default_model.model_id.clone(),
                provider: config.default_model.provider.clone(),
                base_url: None,
            };
            let provider = config
                .providers
                .get(&config.default_model.provider)
                .map(|p| piko_protocol::model::ModelProviderConfig {
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
                thinking_level_map: config.thinking_level_map.clone(),
            })
        };

        let core = Self::new(model_executor, model_config, runtime_config);

        let arc = Arc::new(core);

        // Initialize built-in tool providers (pass sandbox config)
        arc.init(&config.sandbox).await;

        // Auto-register agents from config
        for spec in config.agents.values() {
            arc.register_agent(spec.clone()).await;
        }

        arc
    }
}

// ---- Inherent orchestrator methods (used by TaskControlProvider and RPC) ----

impl OrchCore {
    // ── Agent methods ──

    pub async fn register_agent(&self, spec: AgentSpec) {
        register_agent(self, spec).await;
    }

    pub async fn unregister_agent(&self, agent_id: &str) {
        unregister_agent(self, agent_id.to_string()).await;
    }

    // ── Tool set methods ──

    pub async fn register_tool_set(&self, tool_set: ToolSet) {
        register_tool_set(self, tool_set).await;
    }

    pub async fn unregister_tool_set(&self, tool_set_id: &str) {
        unregister_tool_set(self, tool_set_id.to_string()).await;
    }

    // ── Model config ──

    pub async fn set_model_config(&self, config: OrchModelConfig) {
        set_model_config(self, config).await;
    }

    // ── Provider ──

    pub async fn register_provider(&self, provider: Box<dyn ToolProvider>) {
        register_provider(self, provider).await
    }

    pub async fn unregister_provider(&self, provider_id: &str) {
        self.tool_registry.unregister_provider(provider_id).await;
    }

    // ── Task methods ──

    pub async fn spawn(&self, mut task: AgentTask) -> (AgentTaskId, Option<serde_json::Value>) {
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
        spawn(self, task).await
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
        spawn_detached(self, task).await
    }

    pub async fn await_task(&self, task_id: &str) -> Option<serde_json::Value> {
        await_task(self, task_id.to_string()).await
    }

    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        run(self, prompt.to_string(), opts).await
    }

    pub async fn steer_task(
        &self,
        task_id: &str,
        source_task_id: &str,
        source_agent_id: &str,
        message: &str,
    ) -> bool {
        steer_task(
            self,
            task_id.to_string(),
            source_task_id.to_string(),
            source_agent_id.to_string(),
            message.to_string(),
        )
        .await
    }

    pub async fn cancel_task(&self, task_id: &str, reason: Option<&str>) {
        cancel_task(self, task_id.to_string(), reason.map(|r| r.to_string())).await;
    }

    pub async fn set_approval_gateway(
        &self,
        gateway: Option<Box<dyn crate::ports::approval_gateway::ApprovalGateway>>,
    ) {
        crate::application::tools::set_approval_gateway(self, gateway).await;
    }

    pub async fn subscribe_host_events(
        &self,
        _session_id: String,
        _fallback_agent_id: String,
        listener: Box<dyn Fn(Event) + Send + Sync>,
    ) -> Box<dyn FnOnce() + Send> {
        let wrapped: Arc<dyn Fn(serde_json::Value) + Send + Sync> =
            Arc::new(move |val: serde_json::Value| {
                if let Ok(event) = serde_json::from_value::<Event>(val) {
                    listener(event);
                }
            });

        let id = self
            .next_listener_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        {
            let mut listeners = self.listeners.write().await;
            listeners.insert(id, wrapped);
        }

        let listeners_ref = Arc::clone(&self.listeners);
        Box::new(move || {
            tokio::spawn(async move {
                let mut listeners = listeners_ref.write().await;
                listeners.remove(&id);
            });
        })
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

fn permissive_policy() -> piko_sandbox::policy::Policy {
    piko_sandbox::policy::Policy {
        version: 1,
        read: vec![std::path::PathBuf::from(".")],
        write: vec![std::path::PathBuf::from(".")],
        deny: vec![std::path::PathBuf::from(".git")],
        allowed_commands: vec![
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
        ],
        allow_network: false,
    }
}
