// ---- Orchestrator: task — spawn, spawn_detached, await_task, run, cancel ----

use tokio::sync::oneshot;
use tokio_actors::ActorSystem;

use crate::actors::agent::actor::AgentActor;
use crate::actors::agent::types::{AgentMsg, AgentTaskResultExt};
use crate::protocol::agents::{AgentTask, AgentTaskId, HostTaskContext, TaskSource};
use crate::protocol::host_event::HostEvent;
use crate::protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};

use super::core::OrchCore;

pub(crate) struct PendingDetachedTask {
    pub(crate) receiver: oneshot::Receiver<AgentTaskResultExt>,
    pub(crate) host_context: Option<HostTaskContext>,
    pub(crate) parent_task_id: Option<String>,
}

/// Ensure an agent is registered. If not, register with defaults.
async fn ensure_agent(core: &OrchCore, agent_id: &str) {
    let specs = core.agent_specs.read().await;
    if specs.contains_key(agent_id) {
        return;
    }
    drop(specs);

    let spec = crate::protocol::agents::AgentSpec {
        id: agent_id.to_string(),
        name: agent_id.to_string(),
        role: "assistant".into(),
        description: None,
        system_prompt: String::new(),
        model: None,
        tool_set_ids: vec!["builtin".to_string()],
        active_tool_names: None,
    };
    core.register_agent(spec).await;
}

/// Generate a unique task ID.
fn generate_task_id() -> String {
    format!(
        "task_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(12)
            .collect::<String>()
    )
}

/// Spawn a task on a target agent and block until completion (via oneshot).
pub async fn spawn(core: &OrchCore, mut task: AgentTask) -> AgentTaskId {
    let task_id = task.id.clone().unwrap_or_else(generate_task_id);
    task.id = Some(task_id.clone());
    let target_agent = task.target_agent_id.clone();
    let prompt = task.prompt.clone();
    let parent_task_id = task.parent_task_id.clone();
    let source_agent_id = source_agent_id(&task.source);
    let host_context = task.host_context.clone();

    ensure_agent(core, &target_agent).await;

    {
        let mut allocated = core.allocated_task_ids.write().await;
        allocated.insert(task_id.clone());
    }

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            HostEvent::TaskCreated {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: target_agent.clone(),
                parent_task_id: parent_task_id.clone(),
                source_agent_id,
                prompt,
                turn_id: context.turn_id.clone(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    // Send spawn message via notify + oneshot for deferred result
    let (reply_tx, reply_rx) = oneshot::channel();
    let msg = AgentMsg::Dispatch {
        task: task.clone(),
        reply_tx,
    };

    let handle = ActorSystem::default()
        .get::<AgentActor>(&target_agent)
        .unwrap_or_else(|| panic!("Agent {} not found", target_agent));
    handle
        .send(msg)
        .await
        .expect("Failed to send spawn message");

    // Await deferred result
    if reply_rx.await.is_err()
        && let Some(context) = &host_context
    {
        emit_host_event(
            core,
            HostEvent::TaskFailed {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: target_agent,
                error: "Agent stopped unexpectedly".into(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    task_id
}

/// Non-blocking spawn: starts the task and returns immediately.
pub async fn spawn_detached(core: &OrchCore, mut task: AgentTask) -> AgentTaskId {
    let task_id = task.id.clone().unwrap_or_else(generate_task_id);
    task.id = Some(task_id.clone());
    let target_agent = task.target_agent_id.clone();
    let prompt = task.prompt.clone();
    let parent_task_id = task.parent_task_id.clone();
    let source_agent_id = source_agent_id(&task.source);
    let host_context = task.host_context.clone();

    ensure_agent(core, &target_agent).await;

    {
        let mut allocated = core.allocated_task_ids.write().await;
        allocated.insert(task_id.clone());
    }

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            HostEvent::TaskCreated {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: target_agent.clone(),
                parent_task_id: parent_task_id.clone(),
                source_agent_id,
                prompt,
                turn_id: context.turn_id.clone(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    // Store the oneshot receiver so await_task can collect the result later.
    let (reply_tx, reply_rx) = oneshot::channel();
    {
        let mut pending = core.pending_detached.lock().await;
        pending.insert(
            task_id.clone(),
            PendingDetachedTask {
                receiver: reply_rx,
                host_context: host_context.clone(),
                parent_task_id: parent_task_id.clone(),
            },
        );
    }
    let msg = AgentMsg::Dispatch { task, reply_tx };

    let handle = ActorSystem::default()
        .get::<AgentActor>(&target_agent)
        .unwrap_or_else(|| panic!("Agent {} not found", target_agent));
    handle
        .send(msg)
        .await
        .expect("Failed to send spawn_detached message");

    task_id
}

/// Await the result of a previously detached task.
pub async fn await_task(core: &OrchCore, task_id: String) -> Option<serde_json::Value> {
    let pending_task = {
        let mut pending = core.pending_detached.lock().await;
        pending.remove(&task_id)
    };
    let pending_task = pending_task?;
    match pending_task.receiver.await {
        Ok(result_ext) => {
            let result = serde_json::to_value(&result_ext).unwrap_or_default();
            if let (Some(context), Some(parent_task_id)) =
                (&pending_task.host_context, &pending_task.parent_task_id)
            {
                emit_host_event(
                    core,
                    HostEvent::TaskJoined {
                        session_id: context.session_id.clone(),
                        task_id: task_id.clone(),
                        parent_task_id: parent_task_id.clone(),
                        result: result.clone(),
                        timestamp: now_ms(),
                    },
                )
                .await;
            }
            Some(result)
        }
        Err(_) => None,
    }
}

/// Run a prompt through the orchestrator.
pub async fn run(core: &OrchCore, prompt: String, opts: Option<OrchRunOptions>) -> OrchRunResult {
    let target_agent = opts
        .as_ref()
        .and_then(|o| o.command.target_agent_id.clone())
        .unwrap_or_else(|| core.default_agent_id.clone());

    let task_id = generate_task_id();
    let host_context = opts.as_ref().and_then(|o| o.host_context.clone());
    let task = AgentTask {
        id: Some(task_id.clone()),
        target_agent_id: target_agent.clone(),
        prompt: prompt.clone(),
        source: TaskSource::User,
        priority: None,
        parent_task_id: None,
        history: opts.as_ref().and_then(|o| o.history.clone()),
        host_context: host_context.clone(),
    };

    ensure_agent(core, &task.target_agent_id).await;
    let target = task.target_agent_id.clone();

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            HostEvent::TaskCreated {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: target_agent.clone(),
                parent_task_id: None,
                source_agent_id: None,
                prompt,
                turn_id: context.turn_id.clone(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    {
        let mut allocated = core.allocated_task_ids.write().await;
        allocated.insert(task_id.clone());
    }

    let (reply_tx, reply_rx) = oneshot::channel();
    let msg = AgentMsg::Dispatch { task, reply_tx };

    let handle = ActorSystem::default()
        .get::<AgentActor>(&target)
        .unwrap_or_else(|| panic!("Agent {} not found", target));
    handle.send(msg).await.expect("Failed to send run message");

    match reply_rx.await {
        Ok(result_ext) => OrchRunResult {
            messages: result_ext.messages,
            total_steps: result_ext.total_steps,
            status: RunStatus::Completed,
        },
        Err(_) => OrchRunResult {
            messages: vec![],
            total_steps: 0,
            status: RunStatus::Error,
        },
    }
}

async fn emit_host_event(core: &OrchCore, event: HostEvent) {
    let val = serde_json::to_value(event).unwrap_or_default();
    let listeners = core.listeners.read().await;
    for listener in listeners.values() {
        listener(val.clone());
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn source_agent_id(source: &TaskSource) -> Option<String> {
    match source {
        TaskSource::User => None,
        TaskSource::Agent { agent_id, .. } => Some(agent_id.clone()),
    }
}

/// Push a steering message to a running task.
pub async fn steer_task(
    core: &OrchCore,
    task_id: String,
    source_task_id: String,
    source_agent_id: String,
    message: String,
) -> bool {
    // Find the agent actor that owns this task. Since we don't have a task→agent
    // mapping, iterate over all registered agents and steer each one.
    let specs = core.agent_specs.read().await;
    let mut steered = false;
    for (agent_id, _spec) in specs.iter() {
        let msg = AgentMsg::Steer {
            task_id: task_id.clone(),
            source_task_id: source_task_id.clone(),
            source_agent_id: source_agent_id.clone(),
            message: message.clone(),
        };
        if let Some(handle) = ActorSystem::default().get::<AgentActor>(agent_id) {
            let _ = handle.notify(msg).await;
            steered = true;
        }
    }
    steered
}

/// Cancel a running task.
pub async fn cancel_task(core: &OrchCore, task_id: String, reason: Option<String>) {
    let specs = core.agent_specs.read().await;
    for (agent_id, _spec) in specs.iter() {
        let msg = AgentMsg::Cancel {
            task_id: task_id.clone(),
            reason: reason.clone(),
        };
        if let Some(handle) = ActorSystem::default().get::<AgentActor>(agent_id) {
            let _ = handle.notify(msg).await;
        }
    }
}
