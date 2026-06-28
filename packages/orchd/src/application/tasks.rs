// ---- Application: tasks — spawn, spawn_detached, await_task, run, cancel ----

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::task::{Context, Poll, Waker};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::domain::agents::AgentStatus;
use crate::domain::events::event::Event;
use crate::domain::model::transcript::{ContentBlock, Message};
use crate::domain::model::usage::HostUsage;
use crate::domain::tasks::task::{
    AgentTask, AgentTaskId, AgentTaskResult, AgentTaskState, AgentTaskStatus, HostTaskContext,
    TaskSource,
};
use crate::runtime::agent_stream::agent_loop::run_agent_task;
use crate::runtime::agent_stream::messages::{AgentRunDeps, AgentRuntimeState, AgentTaskResultExt};
use crate::runtime::agent_stream::stream::{AgentStream, AgentEventBuffer};
use piko_protocol::runtime::{OrchRunOptions, OrchRunResult, RunStatus};
use piko_protocol::{ToolCallRef, UsageCost as ProtoUsageCost};

use super::orchestrator::{OrchCore, RunningTaskControl};

pub(crate) struct PendingDetachedTask {
    pub(crate) result: DetachedTaskResult,
    pub(crate) host_context: Option<HostTaskContext>,
    pub(crate) parent_task_id: Option<String>,
}

#[derive(Clone)]
pub(crate) struct DetachedTaskResult {
    inner: Arc<StdMutex<DetachedTaskResultState>>,
}

struct DetachedTaskResultState {
    result: Option<AgentTaskResultExt>,
    waker: Option<Waker>,
}

impl DetachedTaskResult {
    fn new() -> Self {
        Self {
            inner: Arc::new(StdMutex::new(DetachedTaskResultState {
                result: None,
                waker: None,
            })),
        }
    }

    fn complete(&self, result: AgentTaskResultExt) {
        let waker = {
            let mut state = self
                .inner
                .lock()
                .expect("detached task result mutex poisoned");
            state.result = Some(result);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Future for DetachedTaskResult {
    type Output = AgentTaskResultExt;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self
            .inner
            .lock()
            .expect("detached task result mutex poisoned");
        if let Some(result) = state.result.take() {
            Poll::Ready(result)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

struct RunSetup {
    state: Arc<Mutex<AgentRuntimeState>>,
    deps: AgentRunDeps,
    cancel: CancellationToken,
}

/// Ensure an agent is registered. If not, register with defaults.
async fn ensure_agent(core: &OrchCore, agent_id: &str) {
    let specs = core.agent_specs.read().await;
    if specs.contains_key(agent_id) {
        return;
    }
    drop(specs);

    let spec = crate::domain::agents::spec::AgentSpec {
        id: agent_id.to_string(),
        name: agent_id.to_string(),
        role: "assistant".into(),
        description: None,
        system_prompt: String::new(),
        model: None,
        tool_set_ids: vec!["builtin".to_string()],
        active_tool_names: None,
        thinking_level: None,
    };
    core.register_agent(spec).await;
}

async fn setup_run(core: &OrchCore, task: &AgentTask) -> RunSetup {
    let spec = {
        let specs = core.agent_specs.read().await;
        specs
            .get(&task.target_agent_id)
            .cloned()
            .unwrap_or_else(|| panic!("Agent {} not found", task.target_agent_id))
    };

    let model_config = if let Some(model_id) = &spec.model {
        let mut base = core.latest_model_config.read().await.clone();
        if let Some(ref mut mc) = base {
            mc.model.id = model_id.clone();
            mc.model.name = model_id.clone();
        }
        if let Some(ref mut mc) = base
            && let Some(ref thinking) = spec.thinking_level
        {
            mc.settings.thinking_level = Some(thinking.clone());
        }
        base
    } else {
        let mut base = core.latest_model_config.read().await.clone();
        if let Some(ref mut mc) = base
            && let Some(ref thinking) = spec.thinking_level
        {
            mc.settings.thinking_level = Some(thinking.clone());
        }
        base
    };

    let cancel = CancellationToken::new();
    let state = Arc::new(Mutex::new(AgentRuntimeState {
        spec,
        status: AgentStatus::Running,
        current_task_id: task.id.clone(),
        current_parent_task_id: task.parent_task_id.clone(),
        current_host_context: task.host_context.clone(),
        current_run_token: None,
        next_run_token: 1,
        terminal_committed: false,
        abort_token: Some(cancel.clone()),
        steering_queue: Vec::new(),
    }));
    let deps = AgentRunDeps {
        model_executor: Arc::clone(&core.model_executor),
        model_config,
        tool_registry: Arc::clone(&core.tool_registry),
        events: core.event_buffer.read().await.as_ref().cloned().unwrap_or_else(AgentEventBuffer::new),
    };

    RunSetup {
        state,
        deps,
        cancel,
    }
}

async fn execute_agent_task(core: &OrchCore, task: AgentTask) -> AgentTaskResultExt {
    let setup = setup_run(core, &task).await;
    let task_id = task.id.clone().unwrap_or_default();
    let agent_id = task.target_agent_id.clone();

    if let Some(context) = &task.host_context {
        emit_host_event(
            core,
            Event::TaskStarted {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    {
        let mut running = core.running_tasks.lock().await;
        running.insert(
            task_id.clone(),
            RunningTaskControl {
                state: Arc::clone(&setup.state),
                cancel: setup.cancel.clone(),
            },
        );
    }

    let result = match run_agent_task(&setup.state, &setup.deps, task.clone(), setup.cancel).await {
        Ok(result) => result,
        Err(error) => AgentTaskResultExt {
            summary: error.to_string(),
            messages: vec![],
            total_steps: 0,
            final_status: "error".into(),
            artifacts: None,
        },
    };

    {
        let mut running = core.running_tasks.lock().await;
        running.remove(&task_id);
    }

    finalize_task(core, &task, &result).await;
    result
}

async fn finalize_task(core: &OrchCore, task: &AgentTask, result: &AgentTaskResultExt) {
    let Some(context) = &task.host_context else {
        return;
    };

    let task_id = task.id.clone().unwrap_or_else(|| "unknown".into());
    let agent_id = task.target_agent_id.clone();
    match result.final_status.as_str() {
        "aborted" => {
            emit_host_event(
                core,
                Event::TaskCancelled {
                    session_id: context.session_id.clone(),
                    task_id,
                    agent_id,
                    timestamp: now_ms(),
                },
            )
            .await;
        }
        "error" => {
            emit_host_event(
                core,
                Event::TaskFailed {
                    session_id: context.session_id.clone(),
                    task_id,
                    agent_id,
                    error: result.summary.clone(),
                    timestamp: now_ms(),
                },
            )
            .await;
        }
        _ => {
            emit_commit_events(
                core,
                context,
                &task_id,
                &agent_id,
                task.parent_task_id.as_deref().unwrap_or_default(),
                result,
            )
            .await;
            emit_host_event(
                core,
                Event::TaskCompleted {
                    session_id: context.session_id.clone(),
                    task_id,
                    agent_id,
                    total_steps: result.total_steps,
                    summary: result.summary.clone(),
                    final_status: result.final_status.clone(),
                    timestamp: now_ms(),
                },
            )
            .await;
        }
    }
}

async fn emit_commit_events(
    core: &OrchCore,
    context: &HostTaskContext,
    task_id: &str,
    agent_id: &str,
    parent_task_id: &str,
    result: &AgentTaskResultExt,
) {
    let timestamp = now_ms();
    for event in commit_events_from_result(context, task_id, agent_id, result, timestamp) {
        emit_host_event(core, event).await;
    }

    let messages = result
        .messages
        .iter()
        .filter_map(|message| serde_json::to_value(message).ok())
        .collect();
    emit_host_event(
        core,
        Event::TaskTranscriptCommitted {
            session_id: context.session_id.clone(),
            task_id: task_id.to_string(),
            agent_id: agent_id.to_string(),
            parent_task_id: parent_task_id.to_string(),
            messages,
            summary: result.summary.clone(),
            final_status: result.final_status.clone(),
            timestamp,
        },
    )
    .await;
}

fn commit_events_from_result(
    context: &HostTaskContext,
    task_id: &str,
    agent_id: &str,
    result: &AgentTaskResultExt,
    fallback_timestamp: i64,
) -> Vec<Event> {
    let mut events = Vec::new();
    let mut assistant_index = 0u32;

    for message in &result.messages {
        match message {
            Message::Assistant {
                content,
                model,
                provider,
                usage,
                timestamp,
                ..
            } => {
                let message_id = crate::runtime::agent_stream::runtime_assistant_message_id(
                    task_id,
                    &format!("step_{assistant_index}"),
                );
                assistant_index += 1;
                events.push(Event::AssistantMessageCompleted {
                    session_id: context.session_id.clone(),
                    message_id,
                    task_id: task_id.to_string(),
                    agent_id: agent_id.to_string(),
                    text: text_from_blocks(content),
                    tool_calls: tool_calls_from_blocks(content),
                    model: model.clone(),
                    provider: provider.clone(),
                    usage: usage.as_ref().map(host_usage_from_message_usage),
                    timestamp: timestamp.unwrap_or(fallback_timestamp),
                });
            }
            Message::ToolResult {
                tool_call_id,
                tool_name,
                content,
                details,
                is_error,
                timestamp,
            } => {
                events.push(Event::ToolResultCommitted {
                    session_id: context.session_id.clone(),
                    message_id: format!("{task_id}:tool_result:{tool_call_id}"),
                    task_id: task_id.to_string(),
                    agent_id: agent_id.to_string(),
                    tool_call_id: tool_call_id.clone(),
                    tool_name: tool_name.clone().unwrap_or_default(),
                    content: details
                        .clone()
                        .unwrap_or_else(|| serde_json::Value::String(text_from_blocks(content))),
                    is_error: is_error.unwrap_or(false),
                    timestamp: timestamp.unwrap_or(fallback_timestamp),
                });
            }
            _ => {}
        }
    }

    events
}

fn text_from_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn tool_calls_from_blocks(blocks: &[ContentBlock]) -> Vec<ToolCallRef> {
    blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall {
                id,
                name,
                arguments,
                ..
            } => Some(ToolCallRef {
                id: id.clone(),
                name: name.clone(),
                args: arguments.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn host_usage_from_message_usage(
    usage: &crate::domain::model::transcript::MessageUsage,
) -> HostUsage {
    HostUsage {
        input: usage.input,
        output: usage.output,
        cache_read: usage.cache_read,
        cache_write: usage.cache_write,
        total_tokens: usage.total_tokens,
        cost: ProtoUsageCost {
            input: usage.cost.input,
            output: usage.cost.output,
            cache_read: usage.cost.cache_read,
            cache_write: usage.cost.cache_write,
            total: usage.cost.total,
        },
    }
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

/// Spawn a task on a target agent and block until completion.
pub async fn spawn(
    core: &OrchCore,
    mut task: AgentTask,
) -> (AgentTaskId, Option<serde_json::Value>) {
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
    record_task_started(core, &task_id, &task).await;

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            Event::TaskCreated {
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

    let res = execute_agent_task(core, task.clone()).await;
    let val = serde_json::to_value(&res).unwrap_or_default();
    record_task_completed(core, &task_id, res.summary.clone(), res.artifacts.clone()).await;
    if let (Some(context), Some(parent_task_id)) = (&task.host_context, &task.parent_task_id) {
        emit_host_event(
            core,
            Event::TaskJoined {
                session_id: context.session_id.clone(),
                task_id: task_id.clone(),
                parent_task_id: parent_task_id.clone(),
                result: val.clone(),
                timestamp: now_ms(),
            },
        )
        .await;
    }

    (task_id, Some(val))
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
    record_task_started(core, &task_id, &task).await;

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            Event::TaskCreated {
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

    // Store a shared result future so await_task can collect the result later.
    let detached_result = DetachedTaskResult::new();
    {
        let mut pending = core.pending_detached.lock().await;
        pending.insert(
            task_id.clone(),
            PendingDetachedTask {
                result: detached_result.clone(),
                host_context: host_context.clone(),
                parent_task_id: parent_task_id.clone(),
            },
        );
    }
    let task_for_run = task.clone();
    let core = core.clone_for_task();
    let inherited_events = core.event_buffer.read().await.as_ref().cloned();
    let parent_scheduler = core.task_scheduler.read().await.as_ref().cloned();
    if let Some(ref scheduler) = parent_scheduler {
        let events_for_child = inherited_events.clone().unwrap_or_else(AgentEventBuffer::new);
        let child_stream = AgentStream::with_buffers(events_for_child, scheduler.clone(), async move {
            let result = execute_agent_task(&core, task_for_run).await;
            detached_result.complete(result);
        });
        scheduler.push(child_stream);
    } else if let Some(events) = inherited_events {
        // No scheduler, execute inline with buffer
        let child = async move {
            *core.event_buffer.write().await = Some(events.clone());
            let result = execute_agent_task(&core, task_for_run).await;
            detached_result.complete(result);
        };
        child.await;
    } else {
        let child = async move {
            let result = execute_agent_task(&core, task_for_run).await;
            detached_result.complete(result);
        };
        child.await;
    }

    task_id
}

/// Await the result of a previously detached task.
pub async fn await_task(core: &OrchCore, task_id: String) -> Option<serde_json::Value> {
    let pending_task = {
        let mut pending = core.pending_detached.lock().await;
        pending.remove(&task_id)
    };
    let pending_task = pending_task?;
    let result_ext = pending_task.result.await;
    {
        let result = serde_json::to_value(&result_ext).unwrap_or_default();
        record_task_completed(
            core,
            &task_id,
            result_ext.summary.clone(),
            result_ext.artifacts.clone(),
        )
        .await;
        if let (Some(context), Some(parent_task_id)) =
            (&pending_task.host_context, &pending_task.parent_task_id)
        {
            emit_host_event(
                core,
                Event::TaskJoined {
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
}

/// Execute the root task for a run stream.
pub(crate) async fn run_root_task(
    core: &OrchCore,
    prompt: String,
    opts: Option<OrchRunOptions>,
) -> OrchRunResult {
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

    if let Some(context) = &host_context {
        emit_host_event(
            core,
            Event::TaskCreated {
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
    record_task_started(core, &task_id, &task).await;

    let result_ext = execute_agent_task(core, task).await;
    if result_ext.final_status == "error" {
        record_task_failed(core, &task_id, result_ext.summary.clone()).await;
        OrchRunResult {
            messages: result_ext.messages,
            total_steps: result_ext.total_steps,
            status: RunStatus::Error,
        }
    } else {
        record_task_completed(
            core,
            &task_id,
            result_ext.summary.clone(),
            result_ext.artifacts.clone(),
        )
        .await;
        OrchRunResult {
            messages: result_ext.messages,
            total_steps: result_ext.total_steps,
            status: RunStatus::Completed,
        }
    }
}

async fn emit_host_event(core: &OrchCore, event: Event) {
    if let Some(events) = core.event_buffer.read().await.as_ref() {
        events.push(event);
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
    let control = {
        let running = core.running_tasks.lock().await;
        running.get(&task_id).cloned()
    };

    let Some(control) = control else {
        return false;
    };

    let mut state = control.state.lock().await;
    if state.current_task_id.as_deref() != Some(task_id.as_str())
        || state.status != AgentStatus::Running
    {
        return false;
    }

    state
        .steering_queue
        .push(crate::domain::tasks::steering::SteerMessage {
            source_task_id,
            source_agent_id,
            message,
        });
    true
}

/// Cancel a running task.
pub async fn cancel_task(core: &OrchCore, task_id: String, reason: Option<String>) {
    let control = {
        let running = core.running_tasks.lock().await;
        running.get(&task_id).cloned()
    };
    if let Some(control) = control {
        control.cancel.cancel();
        let mut state = control.state.lock().await;
        state.status = AgentStatus::Cancelling;
    }
    record_task_cancelled(core, &task_id, reason).await;
}

impl OrchCore {
    fn clone_for_task(&self) -> Self {
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
            event_buffer: Arc::clone(&self.event_buffer),
            task_scheduler: Arc::clone(&self.task_scheduler),
            pending_detached: Arc::clone(&self.pending_detached),
        }
    }
}

async fn record_task_started(core: &OrchCore, task_id: &str, task: &AgentTask) {
    let mut tasks = core.task_states.write().await;
    tasks.insert(
        task_id.to_string(),
        AgentTaskState {
            id: task_id.to_string(),
            target_agent_id: task.target_agent_id.clone(),
            prompt: task.prompt.clone(),
            source: task.source.clone(),
            status: AgentTaskStatus::Running,
            priority: task.priority.unwrap_or(0),
            parent_task_id: task.parent_task_id.clone(),
            result: None,
            error: None,
            plan: None,
        },
    );
}

async fn record_task_completed(
    core: &OrchCore,
    task_id: &str,
    summary: String,
    artifacts: Option<Vec<crate::domain::tasks::task::AgentArtifact>>,
) {
    let mut tasks = core.task_states.write().await;
    if let Some(task) = tasks.get_mut(task_id) {
        task.status = AgentTaskStatus::Completed;
        task.result = Some(AgentTaskResult { summary, artifacts });
        task.error = None;
    }
}

async fn record_task_failed(core: &OrchCore, task_id: &str, error: String) {
    let mut tasks = core.task_states.write().await;
    if let Some(task) = tasks.get_mut(task_id) {
        task.status = AgentTaskStatus::Failed;
        task.error = Some(error);
    }
}

async fn record_task_cancelled(core: &OrchCore, task_id: &str, reason: Option<String>) {
    let mut tasks = core.task_states.write().await;
    if let Some(task) = tasks.get_mut(task_id) {
        task.status = AgentTaskStatus::Cancelled;
        task.error = reason;
    }
}
