// ---- Orchestrator: task — spawn, spawn_detached, await_task, run, cancel ----

use tokio::sync::oneshot;
use tokio_actors::ActorSystem;

use crate::actors::agent::actor::AgentActor;
use crate::actors::agent::types::AgentMsg;
use crate::protocol::agents::{AgentTask, AgentTaskId, TaskSource};
use crate::protocol::events::{HostEvent, HostOrderBase};
use crate::protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};

use super::core::OrchCore;

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

    ensure_agent(core, &target_agent).await;

    {
        let mut allocated = core.allocated_task_ids.write().await;
        allocated.insert(task_id.clone());
    }

    // Emit task_created
    let created_ev = HostEvent::TaskCreated {
        task: crate::protocol::events::AgentTaskCreated {
            id: task_id.clone(),
            target_agent_id: target_agent.clone(),
            prompt: Some(task.prompt.clone()),
            source: Some(task.source.clone()),
            priority: task.priority,
            parent_task_id: task.parent_task_id.clone(),
            history: task.history.clone(),
        },
    };
    let listeners = core.listeners.read().await;
    for listener in listeners.values() {
        listener(serde_json::to_value(&created_ev).unwrap_or_default());
    }
    drop(listeners);

    // Emit task_started
    let started_ev = HostEvent::TaskStarted {
        order: HostOrderBase::default(),
        agent_id: target_agent.clone(),
        task_id: task_id.clone(),
    };
    let listeners = core.listeners.read().await;
    for listener in listeners.values() {
        listener(serde_json::to_value(&started_ev).unwrap_or_default());
    }
    drop(listeners);

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
    match reply_rx.await {
        Ok(result_ext) => {
            let completed_ev = HostEvent::TaskCompleted {
                order: HostOrderBase::default(),
                agent_id: target_agent.clone(),
                task_id: task_id.clone(),
                result: crate::protocol::agents::AgentTaskResult {
                    summary: result_ext.summary.clone(),
                    artifacts: result_ext.artifacts.clone(),
                },
            };
            let listeners = core.listeners.read().await;
            for listener in listeners.values() {
                listener(serde_json::to_value(&completed_ev).unwrap_or_default());
            }
        }
        Err(_e) => {
            let failed_ev = HostEvent::TaskFailed {
                order: HostOrderBase::default(),
                agent_id: target_agent,
                task_id: task_id.clone(),
                error: "Agent stopped unexpectedly".into(),
            };
            let listeners = core.listeners.read().await;
            for listener in listeners.values() {
                listener(serde_json::to_value(&failed_ev).unwrap_or_default());
            }
        }
    }

    task_id
}

/// Non-blocking spawn: starts the task and returns immediately.
pub async fn spawn_detached(core: &OrchCore, mut task: AgentTask) -> AgentTaskId {
    let task_id = task.id.clone().unwrap_or_else(generate_task_id);
    task.id = Some(task_id.clone());
    let target_agent = task.target_agent_id.clone();

    ensure_agent(core, &target_agent).await;

    {
        let mut allocated = core.allocated_task_ids.write().await;
        allocated.insert(task_id.clone());
    }

    // Store the oneshot receiver so await_task can collect the result later.
    let (reply_tx, reply_rx) = oneshot::channel();
    {
        let mut pending = core.pending_detached.lock().await;
        pending.insert(task_id.clone(), reply_rx);
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
    let rx = {
        let mut pending = core.pending_detached.lock().await;
        pending.remove(&task_id)
    };
    let rx = rx?;
    match rx.await {
        Ok(result_ext) => Some(serde_json::to_value(&result_ext).unwrap_or_default()),
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
    let task = AgentTask {
        id: Some(task_id.clone()),
        target_agent_id: target_agent.clone(),
        prompt,
        source: TaskSource::User,
        priority: None,
        parent_task_id: None,
        history: opts.and_then(|o| o.history),
    };

    ensure_agent(core, &task.target_agent_id).await;
    let target = task.target_agent_id.clone();

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
