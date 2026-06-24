// ---- AgentActor: tokio_actors::Actor implementation ----

use std::sync::Arc;

use tokio::sync::Mutex;
use tokio_actors::{
    ActorResult,
    actor::{Actor, context::ActorContext},
};
use tokio_util::sync::CancellationToken;

use super::agent_loop::start_agent_run;
use super::types::*;

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
                current_run_token: None,
                next_run_token: 1,
                pending_reply_tx: None,
                terminal_committed: false,
                abort_token: None,
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

        state.status = AgentStatus::Running;
        state.current_task_id = Some(task_id.clone());
        state.pending_reply_tx = Some(reply_tx);
        let run_token = state.next_run_token;
        state.next_run_token += 1;
        state.current_run_token = Some(run_token);
        let cancel = CancellationToken::new();
        state.abort_token = Some(cancel.clone());
        state.terminal_committed = false;

        // Emit task_started
        {
            let agent_id = state.spec.id.clone();
            let val = serde_json::json!({
                "type": "task_started",
                "taskId": task_id,
                "agentId": agent_id,
            });
            (self.deps.emit_fn)(agent_id, val).await;
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

        state.current_run_token = None;
        state.status = AgentStatus::Idle;
        state.current_task_id = None;
        state.abort_token = None;

        // Emit events
        match &outcome {
            FinalOutcome::Completed { result } => {
                let val = serde_json::json!({
                    "type": "task_transcript_committed",
                    "agentId": agent_id,
                    "taskId": task_id,
                    "messages": result.messages,
                    "summary": result.summary,
                    "finalStatus": result.final_status,
                });
                (self.deps.emit_fn)(agent_id.clone(), val).await;

                let final_type = if result.final_status == "aborted" {
                    "task_cancelled"
                } else if result.final_status == "error" {
                    "task_failed"
                } else {
                    "task_completed"
                };
                let final_val = serde_json::json!({
                    "type": final_type,
                    "agentId": agent_id,
                    "taskId": task_id,
                    "summary": result.summary,
                });
                (self.deps.emit_fn)(agent_id.clone(), final_val).await;
            }
            FinalOutcome::Failed { error } => {
                let val = serde_json::json!({
                    "type": "task_failed",
                    "agentId": agent_id,
                    "taskId": task_id,
                    "error": error,
                });
                (self.deps.emit_fn)(agent_id.clone(), val).await;
            }
            FinalOutcome::Cancelled { reason } => {
                let val = serde_json::json!({
                    "type": "task_cancelled",
                    "agentId": agent_id,
                    "taskId": task_id,
                    "reason": reason,
                });
                (self.deps.emit_fn)(agent_id.clone(), val).await;
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
