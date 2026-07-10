use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::agents::spec::AgentSpec;
use crate::domain::model::step::{ModelConfig, ModelRunSettings, ModelSpec};
use crate::domain::model::transcript::TranscriptManager;
use crate::runtime::dispatch::DispatchSenders;
use crate::runtime::tool_executor;
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
        senders: Option<DispatchSenders>,
        message_id: String,
        cancel: CancellationToken,
        transcript: &mut TranscriptManager,
        step_count: u32,
        tool_calls: &[ToolCallItem],
        routes: &HashMap<String, CatalogRoute>,
    ) -> Result<tool_executor::ToolExecutionResult, String> {
        let tool_consumer = task_context.tool_execution_consumer(senders, message_id);
        tool_consumer
            .execute_tool_calls(
                &self.deps,
                tool_calls,
                routes,
                &self.model_settings,
                cancel,
                transcript,
                step_count,
            )
            .await
    }
}
