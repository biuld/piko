// ---- Application: OrchCore — thin orchestrator facade ----
//
// OrchCore is the central orchestrator struct. It holds all orchestrator state
// and exposes methods for hostd to manage agents, tasks, and tool execution.
//
// This is a thin facade over domain entities, ports, and adapters.

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::ToolRegistryImpl;
use crate::adapters::tools::task_control_provider::TaskControlProvider;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelConfig;
use crate::domain::tasks::steering::SteerMessage;
use crate::domain::tasks::task::{AgentTask, AgentTaskId, AgentTaskState};
use crate::domain::tools::definition::ToolSet;
use crate::ports::model_gateway::LlmGateway;
use crate::ports::tool_provider::ToolProvider;
use crate::runtime::agent_stream::messages::AgentRuntimeState;
use crate::runtime::agent_stream::stream::{RunContext, root_agent_stream};
use futures_core::Stream;
use futures_util::StreamExt;
use piko_protocol::Event;
use piko_protocol::config::{OrchdConfig, SandboxConfig};
use piko_protocol::runtime::{
    GraphSnapshot, OrchModelConfig, OrchRunOptions, OrchRunResult, OrchestratorRuntimeConfig,
    RunStatus,
};
use piko_protocol::state::OrchState;

use super::agents::{register_agent, unregister_agent};
use super::snapshots::{get_graph, snapshot, update_plan};
use super::tasks::{
    PendingDetachedTask, await_task, cancel_task, spawn, spawn_detached, steer_task,
};
use super::tools::{register_provider, register_tool_set, set_model_config, unregister_tool_set};

// ---- OrchCore struct ----

/// Central orchestrator runtime.
///
/// Holds runtime state: tool registry, agent specs, model executor,
/// task projections, stream state, and pending detached task results.
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
    pub(crate) running_tasks: Arc<Mutex<HashMap<String, RunningTaskControl>>>,
    pub(crate) steer_tx: Arc<RwLock<Option<mpsc::UnboundedSender<SteerMessage>>>>,
    pub(crate) child_tx: Arc<RwLock<Option<mpsc::UnboundedSender<Event>>>>,
    /// Pending result futures for detached tasks, keyed by task_id.
    pub(crate) pending_detached: Arc<Mutex<HashMap<String, PendingDetachedTask>>>,
}

#[derive(Clone)]
pub(crate) struct RunningTaskControl {
    pub(crate) state: Arc<Mutex<AgentRuntimeState>>,
    pub(crate) cancel: CancellationToken,
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

        let tool_registry = Arc::new(ToolRegistryImpl::new());

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
            running_tasks: Arc::new(Mutex::new(HashMap::new())),
            steer_tx: Arc::new(RwLock::new(None)),
            child_tx: Arc::new(RwLock::new(None)),
            pending_detached: Arc::new(Mutex::new(HashMap::new())),
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
            tools: vec![
                crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                    provider_id: "orch".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                },
            ],
        };
        self.tool_registry.register_tool_set(builtin_toolset).await;

        let workspace_toolset = ToolSet {
            id: "workspace".into(),
            name: "Workspace Tools".into(),
            description: Some("Local workspace tools".into()),
            metadata: None,
            policy: None,
            tools: vec![
                crate::domain::tools::definition::ToolSetToolRef::ProviderNamespace {
                    provider_id: "workspace".into(),
                    namespace: "".into(),
                    alias: None,
                    policy: None,
                },
            ],
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
        let mut stream = self
            .run_streaming(prompt, Some(ensure_run_context(opts)))
            .await;
        let mut messages = Vec::new();
        let mut total_steps = 0;
        let mut status = RunStatus::Completed;

        while let Some(event) = stream.next().await {
            match event {
                Event::TaskTranscriptCommitted {
                    messages: committed,
                    final_status,
                    ..
                } => {
                    messages = committed
                        .into_iter()
                        .filter_map(|message| serde_json::from_value(message).ok())
                        .collect();
                    status = run_status_from_final_status(&final_status);
                }
                Event::TaskCompleted {
                    total_steps: steps,
                    final_status,
                    ..
                } => {
                    total_steps = steps;
                    status = run_status_from_final_status(&final_status);
                }
                Event::TaskFailed { .. } => {
                    status = RunStatus::Error;
                }
                Event::TaskCancelled { .. } => {
                    status = RunStatus::Aborted;
                }
                _ => {}
            }
        }

        OrchRunResult {
            messages,
            total_steps,
            status,
        }
    }

    /// Run a prompt and return the host-facing event stream for this run.
    pub async fn run_streaming(&self, prompt: &str, opts: Option<OrchRunOptions>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let target_agent = opts.as_ref().and_then(|o| o.command.target_agent_id.clone())
            .unwrap_or_else(|| self.default_agent_id.clone());
        let task_id = format!("task_{}", uuid::Uuid::new_v4().to_string().chars().take(12).collect::<String>());
        let host_context = opts.as_ref().and_then(|o| o.host_context.clone());

        let spec = self.agent_specs.read().await.get(&target_agent).cloned()
            .unwrap_or_else(|| AgentSpec { id: target_agent.clone(), name: target_agent.clone(), role: "assistant".into(), description: None, system_prompt: String::new(), model: None, tool_set_ids: vec!["builtin".into(), "workspace".into()], active_tool_names: None, thinking_level: None });

        let task = AgentTask { id: Some(task_id.clone()), target_agent_id: target_agent.clone(), prompt: prompt.to_string(), source: crate::domain::tasks::task::TaskSource::User, priority: None, parent_task_id: None, history: opts.as_ref().and_then(|o| o.history.clone()), host_context: host_context.clone() };

        let deps = crate::runtime::agent_stream::messages::AgentRunDeps { model_executor: Arc::clone(&self.model_executor), model_config: self.latest_model_config.read().await.clone(), tool_registry: Arc::clone(&self.tool_registry) };

        let (steer_tx, steer_rx) = mpsc::unbounded_channel();
        let (child_tx, child_rx) = mpsc::unbounded_channel();
        let ctx = RunContext { steer_tx: steer_tx.clone(), child_tx: child_tx.clone(), cancel: CancellationToken::new() };

        *self.steer_tx.write().await = Some(steer_tx);
        *self.child_tx.write().await = Some(child_tx);

        Box::pin(root_agent_stream(ctx, steer_rx, child_rx, deps, task, spec))
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

impl OrchCore {
    fn clone_for_streaming(&self) -> Self {
        Self {
            run_id: self.run_id.clone(),
            tool_registry: Arc::clone(&self.tool_registry),
            model_executor: Arc::clone(&self.model_executor),
            latest_model_config: Arc::clone(&self.latest_model_config),
            default_agent_id: self.default_agent_id.clone(),
            agent_specs: Arc::clone(&self.agent_specs),
            task_states: Arc::clone(&self.task_states),
            allocated_task_ids: Arc::clone(&self.allocated_task_ids),
            _max_concurrent_agents: self._max_concurrent_agents,
            running_tasks: Arc::clone(&self.running_tasks),
            steer_tx: Arc::clone(&self.steer_tx),
            child_tx: Arc::clone(&self.child_tx),
            pending_detached: Arc::clone(&self.pending_detached),
        }
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

fn run_status_from_final_status(status: &str) -> RunStatus {
    match status {
        "aborted" | "cancelled" => RunStatus::Aborted,
        "error" | "failed" => RunStatus::Error,
        _ => RunStatus::Completed,
    }
}

fn ensure_run_context(opts: Option<OrchRunOptions>) -> OrchRunOptions {
    let mut opts = opts.unwrap_or(OrchRunOptions {
        command: piko_protocol::runtime::OrchRunCommandOptions {
            target_agent_id: None,
        },
        history: None,
        host_context: None,
    });
    if opts.host_context.is_none() {
        let id = uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>();
        opts.host_context = Some(piko_protocol::agents::HostTaskContext {
            session_id: format!("run_compat_{id}"),
            turn_id: format!("turn_compat_{id}"),
        });
    }
    opts
}
