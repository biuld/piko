// ---- Run — streaming and synchronous run methods ----

use std::pin::Pin;
use std::sync::Arc;

use futures_core::Stream;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::spec::AgentSpec;
use crate::domain::tasks::task::{AgentTask, HostTaskContext, TaskSource};
use crate::ports::agent_spawner::AgentSpawner;
use crate::runtime::stream::AgentRunDeps;
use crate::runtime::stream::{self, RunContext};
use piko_protocol::Event;
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};

use super::supervisor::Supervisor;
use super::utils::{ensure_run_context, generate_task_id, run_status_from_final_status};

impl Supervisor {
    /// Run a prompt and return the host-facing event stream.
    pub async fn run_streaming(
        &self,
        prompt: &str,
        opts: Option<OrchRunOptions>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let target_agent = if let Some(aid) = opts
            .as_ref()
            .and_then(|o| o.command.target_agent_id.clone())
        {
            aid
        } else {
            self.state.default_agent_id.read().await.clone()
        };
        let task_id = format!(
            "task_{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(12)
                .collect::<String>()
        );
        let host_context = opts.as_ref().and_then(|o| o.host_context.clone());

        let spec = self
            .state
            .agent_specs
            .read()
            .await
            .get(&target_agent)
            .cloned()
            .unwrap_or_else(|| AgentSpec {
                id: target_agent.clone(),
                name: target_agent.clone(),
                role: "assistant".into(),
                description: None,
                system_prompt: String::new(),
                model: None,
                tool_set_ids: vec!["builtin".into(), "workspace".into()],
                active_tool_names: None,
                thinking_level: None,
            });

        let task = AgentTask {
            id: Some(task_id.clone()),
            target_agent_id: target_agent.clone(),
            prompt: prompt.to_string(),
            source: TaskSource::User,
            priority: None,
            parent_task_id: None,
            history: opts.as_ref().and_then(|o| o.history.clone()),
            host_context: host_context.clone(),
        };

        let deps = AgentRunDeps {
            model_executor: Arc::clone(&self.state.model_executor),
            model_config: self.state.model_config.read().await.clone(),
            tool_registry: Arc::clone(&self.state.tool_registry),
        };

        let (steer_tx, steer_rx) = mpsc::unbounded_channel();
        let ctx = RunContext {
            steer_tx: steer_tx.clone(),
            cancel: CancellationToken::new(),
        };

        *self.state.steer_tx.write().await = Some(steer_tx);

        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });
        Box::pin(stream::agent_loop(ctx, steer_rx, deps, task, spec, spawner))
    }

    /// Run a prompt synchronously (drains the stream).
    pub async fn run(&self, prompt: &str, opts: Option<OrchRunOptions>) -> OrchRunResult {
        let stream = self
            .run_streaming(prompt, Some(ensure_run_context(opts)))
            .await;
        let mut total_steps = 0;
        let mut status = RunStatus::Completed;

        tokio::pin!(stream);
        while let Some(event) = stream.next().await {
            match event {
                Event::TaskCompleted {
                    total_steps: steps,
                    final_status,
                    ..
                } => {
                    total_steps = steps;
                    status = run_status_from_final_status(&final_status);
                }
                Event::TaskFailed { .. } => status = RunStatus::Error,
                Event::TaskCancelled { .. } => status = RunStatus::Aborted,
                _ => {}
            }
        }

        OrchRunResult {
            messages: vec![],
            total_steps,
            status,
        }
    }

    /// Spawn the root agent and return its event stream.
    pub async fn spawn_root_agent(
        &self,
        spec: AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        self.spawn_agent_stream(spec, prompt, host_context, None)
            .await
    }

    /// Internal: create an agent stream and wire it into the DAG.
    pub(crate) async fn spawn_agent_stream(
        &self,
        spec: AgentSpec,
        prompt: String,
        host_context: Option<HostTaskContext>,
        parent_agent_id: Option<piko_protocol::AgentId>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let agent_id = spec.id.clone();
        let task_id = generate_task_id();
        let cancel = CancellationToken::new();
        let (steer_tx, steer_rx) = tokio::sync::mpsc::unbounded_channel();

        self.state
            .dag
            .write()
            .await
            .insert(agent_id.clone(), parent_agent_id.clone());
        self.state.handles.write().await.insert(
            agent_id.clone(),
            super::supervisor::AgentHandle {
                agent_id: agent_id.clone(),
                parent_agent_id: parent_agent_id.clone(),
                cancel: cancel.clone(),
                steer_tx: steer_tx.clone(),
            },
        );

        let task = AgentTask {
            id: Some(task_id),
            target_agent_id: agent_id,
            prompt,
            source: TaskSource::User,
            priority: None,
            parent_task_id: None,
            history: None,
            host_context,
        };

        let deps = AgentRunDeps {
            model_executor: Arc::clone(&self.state.model_executor),
            model_config: self.state.model_config.read().await.clone(),
            tool_registry: Arc::clone(&self.state.tool_registry),
        };

        let ctx = RunContext {
            steer_tx: steer_tx.clone(),
            cancel: cancel.clone(),
        };

        let spawner: Arc<dyn AgentSpawner> = Arc::new(Self {
            state: Arc::clone(&self.state),
        });

        Box::pin(stream::agent_loop(ctx, steer_rx, deps, task, spec, spawner))
    }
}
