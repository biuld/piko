use crate::adapters::tools::registry::ToolRegistry;
use crate::domain::Event;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::ModelSpec;
use crate::domain::transcript::Message;
use crate::runtime::step::StepDispatchResult;

use super::{AgentRunDeps, RunContext, context::TaskContext, state::TaskRunState};

pub(super) struct StepCycle {
    pub(super) result: StepDispatchResult,
    pub(super) routes:
        std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
    pub(super) model: ModelSpec,
    pub(super) message_id: String,
}

pub(super) struct PendingToolExecution {
    pub(super) tool_calls: Vec<crate::domain::tools::call::ToolCallItem>,
    pub(super) routes:
        std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
    pub(super) message_id: String,
}

pub(super) struct AppliedStep {
    pub(super) assistant_message: Message,
    pub(super) tool_calls: Vec<crate::domain::tools::call::ToolCallItem>,
    pub(super) routes:
        std::collections::HashMap<String, crate::adapters::tools::registry::CatalogRoute>,
    pub(super) message_id: String,
    pub(super) events: Vec<Event>,
}

pub(super) enum StepAdvance {
    AwaitNextTurn {
        events: Vec<Event>,
        summary: String,
    },
    Stop {
        events: Vec<Event>,
    },
    ExecuteTools {
        events: Vec<Event>,
        pending: PendingToolExecution,
    },
}

pub(super) struct StepDispatchFailure {
    pub(super) error: String,
    pub(super) result: StepDispatchResult,
}

pub(super) async fn run_step_cycle(
    deps: &AgentRunDeps,
    ctx: &RunContext,
    spec: &AgentSpec,
    task_context: &TaskContext,
    run_state: &mut TaskRunState,
    current_model: ModelSpec,
) -> Result<StepCycle, StepDispatchFailure> {
    let step_count = run_state.begin_step();
    let step_id = format!("step_{}", step_count);
    let message_id = task_context.assistant_message_id(step_count);
    let (tools, routes) = (*deps.tool_registry)
        .discover_tools(&task_context.tool_discovery_context(spec))
        .await;
    let request = task_context.gateway_request(
        deps,
        spec,
        run_state.transcript(),
        &current_model,
        step_id,
        tools,
    );

    let result = match deps
        .model_executor
        .chat_stream(request, Some(ctx.cancel.clone()))
        .await
    {
        Ok(llm) => {
            let work_id = run_state
                .active_work_id()
                .map(str::to_string)
                .unwrap_or_else(|| "work_unknown".to_string());
            let mut dispatch = task_context.step_dispatch(
                message_id.clone(),
                work_id.clone(),
                current_model.clone(),
                llm,
            );
            let emitter = run_state.event_emitter(task_context.dispatch_identity(), work_id);
            let result = dispatch.dispatch_step(Some(&emitter)).await;
            drop(dispatch);
            if let Some(error) = emitter.take_persist_error() {
                Err(StepDispatchFailure { error, result })
            } else {
                Ok(result)
            }
        }
        Err(error) => {
            let work_id = run_state
                .active_work_id()
                .map(str::to_string)
                .unwrap_or_else(|| "work_unknown".to_string());
            let mut dispatch = task_context.step_failure_dispatch(
                message_id.clone(),
                work_id.clone(),
                current_model.clone(),
                error.to_string(),
            );
            let emitter = run_state.event_emitter(task_context.dispatch_identity(), work_id);
            let result = dispatch.dispatch_step(Some(&emitter)).await;
            drop(dispatch);
            Err(StepDispatchFailure {
                error: error.to_string(),
                result,
            })
        }
    }?;

    Ok(StepCycle {
        result,
        routes,
        model: current_model,
        message_id,
    })
}
