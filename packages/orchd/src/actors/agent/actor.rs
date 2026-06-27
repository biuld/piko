// ---- AgentActor: tokio_actors::Actor implementation ----

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_actors::{
    ActorResult,
    actor::{Actor, context::ActorContext},
};
use tokio_util::sync::CancellationToken;

use super::agent_loop::start_agent_run;
use super::types::{SteerMessage, *};
use crate::model::types::runtime_assistant_message_id;
use crate::protocol::host_event::{
    HostEvent, ToolCallRef, Usage as HostUsage, UsageCost as HostUsageCost,
};
use crate::protocol::messages::{ContentBlock, Message, Usage as MessageUsage};

// ---- Final outcome ----

enum FinalOutcome {
    Completed { result: AgentTaskResultExt },
    Failed { error: String },
    Cancelled { reason: String },
}

// ---- AgentActor ----

pub struct AgentActor {
    state: Arc<Mutex<AgentRuntimeState>>,
    deps: AgentActorDeps,
}

impl AgentActor {
    pub fn new(spec: crate::protocol::agents::AgentSpec, deps: AgentActorDeps) -> Self {
        Self {
            state: Arc::new(Mutex::new(AgentRuntimeState {
                spec,
                status: AgentStatus::Idle,
                current_task_id: None,
                current_parent_task_id: None,
                current_host_context: None,
                current_run_token: None,
                next_run_token: 1,
                pending_reply_tx: None,
                terminal_committed: false,
                abort_token: None,
                steering_queue: Vec::new(),
            })),
            deps,
        }
    }
}

impl Actor for AgentActor {
    type Message = AgentMsg;
    type Response = AgentTaskResultExt;

    async fn handle(
        &mut self,
        msg: AgentMsg,
        ctx: &mut ActorContext<Self>,
    ) -> ActorResult<AgentTaskResultExt> {
        match msg {
            AgentMsg::Dispatch { task, reply_tx } => {
                self.handle_dispatch(task, reply_tx, ctx).await
            }
            AgentMsg::RunnerFinished {
                task_id,
                token,
                result,
            } => {
                self.handle_runner_finished(task_id, token, result, ctx)
                    .await
            }
            AgentMsg::RunnerFailed {
                task_id,
                token,
                error,
            } => self.handle_runner_failed(task_id, token, error, ctx).await,
            AgentMsg::Cancel { task_id, reason } => self.handle_cancel(task_id, reason).await,
            AgentMsg::Wake => Ok(AgentTaskResultExt::default()),
            AgentMsg::SetModelConfig { config } => {
                self.handle_set_model_config(config);
                Ok(AgentTaskResultExt::default())
            }
            AgentMsg::Steer {
                task_id,
                source_task_id,
                source_agent_id,
                message,
            } => {
                self.handle_steer(task_id, source_task_id, source_agent_id, message)
                    .await;
                Ok(AgentTaskResultExt::default())
            }
        }
    }
}

impl AgentActor {
    async fn handle_dispatch(
        &mut self,
        task: crate::protocol::agents::AgentTask,
        reply_tx: tokio::sync::oneshot::Sender<AgentTaskResultExt>,
        ctx: &mut ActorContext<Self>,
    ) -> ActorResult<AgentTaskResultExt> {
        let mut state = self.state.lock().await;

        if state.status == AgentStatus::Running {
            // Reply immediately with error
            let _ = reply_tx.send(AgentTaskResultExt {
                summary: "Agent already running a task".into(),
                messages: vec![],
                total_steps: 0,
                final_status: "error".into(),
                artifacts: None,
            });
            return Ok(AgentTaskResultExt::default());
        }

        let task_id = task.id.clone().unwrap_or_default();
        let host_context = task.host_context.clone();

        state.status = AgentStatus::Running;
        state.current_task_id = Some(task_id.clone());
        state.current_parent_task_id = task.parent_task_id.clone();
        state.current_host_context = host_context.clone();
        state.pending_reply_tx = Some(reply_tx);
        let run_token = state.next_run_token;
        state.next_run_token += 1;
        state.current_run_token = Some(run_token);
        let cancel = CancellationToken::new();
        state.abort_token = Some(cancel.clone());
        state.terminal_committed = false;

        if let Some(context) = host_context {
            let agent_id = state.spec.id.clone();
            emit_host(
                &self.deps,
                HostEvent::TaskStarted {
                    session_id: context.session_id,
                    task_id: task_id.clone(),
                    agent_id,
                    timestamp: now_ms(),
                },
            )
            .await;
        }

        drop(state);

        // Get self handle for the engine loop to send RunnerFinished back
        let self_handle = ctx.self_handle().clone();

        start_agent_run(
            self.state.clone(),
            self.deps.clone(),
            self_handle,
            task,
            run_token,
            cancel,
        );

        // Don't return the final result yet — it comes via RunnerFinished
        Ok(AgentTaskResultExt::default())
    }

    async fn handle_runner_finished(
        &mut self,
        task_id: String,
        token: u64,
        result: AgentTaskResultExt,
        _ctx: &mut ActorContext<Self>,
    ) -> ActorResult<AgentTaskResultExt> {
        let state = self.state.lock().await;
        if state.current_run_token != Some(token)
            || state.current_task_id.as_deref() != Some(&task_id)
        {
            return Ok(AgentTaskResultExt::default());
        }
        drop(state);

        self.finalize(FinalOutcome::Completed { result }).await;
        Ok(AgentTaskResultExt::default())
    }

    async fn handle_runner_failed(
        &mut self,
        task_id: String,
        token: u64,
        error: String,
        _ctx: &mut ActorContext<Self>,
    ) -> ActorResult<AgentTaskResultExt> {
        let state = self.state.lock().await;
        if state.current_run_token != Some(token)
            || state.current_task_id.as_deref() != Some(&task_id)
        {
            return Ok(AgentTaskResultExt::default());
        }
        let is_cancelling = state.status == AgentStatus::Cancelling;
        drop(state);

        if is_cancelling {
            self.finalize(FinalOutcome::Cancelled { reason: error })
                .await;
        } else {
            self.finalize(FinalOutcome::Failed { error }).await;
        }
        Ok(AgentTaskResultExt::default())
    }

    async fn handle_cancel(
        &mut self,
        task_id: String,
        _reason: Option<String>,
    ) -> ActorResult<AgentTaskResultExt> {
        let mut state = self.state.lock().await;
        if state.current_task_id.as_deref() == Some(&task_id)
            && state.status == AgentStatus::Running
        {
            state.status = AgentStatus::Cancelling;
            if let Some(token) = &state.abort_token {
                token.cancel();
            }
        }
        Ok(AgentTaskResultExt::default())
    }

    fn handle_set_model_config(&mut self, config: ModelConfig) {
        // Update model config at runtime
        self.deps.model_config = Some(config);
    }

    async fn handle_steer(
        &mut self,
        task_id: String,
        source_task_id: String,
        source_agent_id: String,
        message: String,
    ) {
        let mut state = self.state.lock().await;
        // Only accept steering for the currently running task
        if state.current_task_id.as_deref() == Some(&task_id)
            && state.status == AgentStatus::Running
        {
            state.steering_queue.push(SteerMessage {
                source_task_id,
                source_agent_id,
                message,
            });
        }
    }

    // ---- Finalize ----

    async fn finalize(&mut self, outcome: FinalOutcome) {
        let mut state = self.state.lock().await;
        if state.terminal_committed {
            return;
        }
        state.terminal_committed = true;

        let task_id = state
            .current_task_id
            .clone()
            .unwrap_or_else(|| "unknown".into());
        let agent_id = state.spec.id.clone();
        let host_context = state.current_host_context.clone();
        let parent_task_id = state.current_parent_task_id.clone().unwrap_or_default();

        state.current_run_token = None;
        state.status = AgentStatus::Idle;
        state.current_task_id = None;
        state.current_parent_task_id = None;
        state.current_host_context = None;
        state.abort_token = None;

        if let Some(context) = &host_context {
            match &outcome {
                FinalOutcome::Completed { result } if result.final_status == "aborted" => {
                    emit_host(
                        &self.deps,
                        HostEvent::TaskCancelled {
                            session_id: context.session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: agent_id.clone(),
                            timestamp: now_ms(),
                        },
                    )
                    .await;
                }
                FinalOutcome::Completed { result } if result.final_status == "error" => {
                    emit_host(
                        &self.deps,
                        HostEvent::TaskFailed {
                            session_id: context.session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: agent_id.clone(),
                            error: result.summary.clone(),
                            timestamp: now_ms(),
                        },
                    )
                    .await;
                }
                FinalOutcome::Completed { result } => {
                    emit_commit_events(
                        &self.deps,
                        context,
                        &task_id,
                        &agent_id,
                        &parent_task_id,
                        result,
                    )
                    .await;
                    emit_host(
                        &self.deps,
                        HostEvent::TaskCompleted {
                            session_id: context.session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: agent_id.clone(),
                            total_steps: result.total_steps,
                            summary: result.summary.clone(),
                            final_status: result.final_status.clone(),
                            timestamp: now_ms(),
                        },
                    )
                    .await;
                }
                FinalOutcome::Failed { error } => {
                    emit_host(
                        &self.deps,
                        HostEvent::TaskFailed {
                            session_id: context.session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: agent_id.clone(),
                            error: error.clone(),
                            timestamp: now_ms(),
                        },
                    )
                    .await;
                }
                FinalOutcome::Cancelled { .. } => {
                    emit_host(
                        &self.deps,
                        HostEvent::TaskCancelled {
                            session_id: context.session_id.clone(),
                            task_id: task_id.clone(),
                            agent_id: agent_id.clone(),
                            timestamp: now_ms(),
                        },
                    )
                    .await;
                }
            }
        }

        // Reply to the Dispatch caller via oneshot
        if let Some(reply_tx) = state.pending_reply_tx.take() {
            let reply_val = match outcome {
                FinalOutcome::Completed { result } => result,
                FinalOutcome::Failed { error } => AgentTaskResultExt {
                    summary: error,
                    messages: vec![],
                    total_steps: 0,
                    final_status: "error".into(),
                    artifacts: None,
                },
                FinalOutcome::Cancelled { reason } => AgentTaskResultExt {
                    summary: reason,
                    messages: vec![],
                    total_steps: 0,
                    final_status: "aborted".into(),
                    artifacts: None,
                },
            };
            let _ = reply_tx.send(reply_val);
        }
    }
}

async fn emit_host(deps: &AgentActorDeps, event: HostEvent) {
    let val = serde_json::to_value(event).unwrap_or_default();
    (deps.emit_fn)(String::new(), val).await;
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

async fn emit_commit_events(
    deps: &AgentActorDeps,
    context: &crate::protocol::agents::HostTaskContext,
    task_id: &str,
    agent_id: &str,
    parent_task_id: &str,
    result: &AgentTaskResultExt,
) {
    let timestamp = now_ms();
    for event in commit_events_from_result(context, task_id, agent_id, result, timestamp) {
        emit_host(deps, event).await;
    }

    let messages = result
        .messages
        .iter()
        .filter_map(|message| serde_json::to_value(message).ok())
        .collect();
    emit_host(
        deps,
        HostEvent::TaskTranscriptCommitted {
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
    context: &crate::protocol::agents::HostTaskContext,
    task_id: &str,
    agent_id: &str,
    result: &AgentTaskResultExt,
    fallback_timestamp: i64,
) -> Vec<HostEvent> {
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
                let message_id =
                    runtime_assistant_message_id(task_id, &format!("step_{assistant_index}"));
                assistant_index += 1;
                events.push(HostEvent::AssistantMessageCompleted {
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
                events.push(HostEvent::ToolResultCommitted {
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
            Message::User { .. } => {}
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

fn host_usage_from_message_usage(usage: &MessageUsage) -> HostUsage {
    HostUsage {
        input: usage.input,
        output: usage.output,
        cache_read: usage.cache_read,
        cache_write: usage.cache_write,
        total_tokens: usage.total_tokens,
        cost: HostUsageCost {
            input: usage.cost.input,
            output: usage.cost.output,
            cache_read: usage.cost.cache_read,
            cache_write: usage.cost.cache_write,
            total: usage.cost.total,
        },
    }
}
