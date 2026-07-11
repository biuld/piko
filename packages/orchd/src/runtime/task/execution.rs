use std::collections::HashMap;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::{ModelConfig, ModelRunSettings, ModelSpec};
use crate::integration::PersistSink;
use crate::runtime::tools;
use crate::runtime::types::ToolCallItem;

use super::context::TaskContext;
use super::run_state::TaskRunState;
use super::step::{self, StepCycle, StepDispatchFailure};
use super::{AgentRunDeps, RunContext};

pub(super) struct TaskExecution {
    deps: AgentRunDeps,
    spec: AgentSpec,
    model_settings: ModelRunSettings,
    model_config: Option<ModelConfig>,
}

impl TaskExecution {
    pub(super) fn new(deps: AgentRunDeps, spec: AgentSpec) -> Self {
        let model_settings = deps
            .model_config
            .as_ref()
            .map(|c| c.settings.clone())
            .unwrap_or(ModelRunSettings {
                allow_tool_calls: true,
                ..Default::default()
            });
        let model_config = deps.model_config.clone();

        Self {
            deps,
            spec,
            model_settings,
            model_config,
        }
    }

    pub(super) fn allow_tool_calls(&self) -> bool {
        self.model_settings.allow_tool_calls
    }

    pub(super) fn current_model(&self) -> ModelSpec {
        self.model_config
            .as_ref()
            .map(|c| c.model.clone())
            .unwrap_or(ModelSpec {
                id: "default".into(),
                name: "Default".into(),
                provider: "openai".into(),
            })
    }

    pub(super) async fn run_step_cycle(
        &self,
        ctx: &RunContext,
        task_context: &TaskContext,
        run_state: &mut TaskRunState,
    ) -> Result<StepCycle, StepDispatchFailure> {
        step::run_step_cycle(
            &self.deps,
            ctx,
            &self.spec,
            task_context,
            run_state,
            self.current_model(),
        )
        .await
    }

    pub(super) async fn execute_tool_calls(
        &self,
        task_context: &TaskContext,
        run_state: &mut TaskRunState,
        message_id: String,
        cancel: CancellationToken,
        step_count: u32,
        tool_calls: &[ToolCallItem],
        routes: &HashMap<String, CatalogRoute>,
    ) -> Result<tools::ToolExecutionResult, String> {
        let work_id = run_state
            .active_work_id()
            .map(str::to_string)
            .unwrap_or_else(|| "work_unknown".to_string());
        let emitter = run_state.event_emitter(task_context.dispatch_identity(), work_id.clone());
        let tool_consumer = task_context.tool_execution_consumer(
            emitter,
            message_id,
            work_id,
            run_state.active_source_turn_id().map(str::to_string),
        );
        tool_consumer
            .execute_tool_calls(
                &self.deps,
                tool_calls,
                routes,
                &self.model_settings,
                cancel,
                run_state.transcript_mut(),
                step_count,
            )
            .await
    }

    pub(super) fn persist_sink(&self) -> Option<Arc<dyn PersistSink>> {
        self.deps.persist_sink.clone()
    }
}
