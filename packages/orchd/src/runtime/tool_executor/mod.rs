use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::events::event::Event;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::model::transcript::TranscriptManager;
use crate::domain::tools::definition::ToolExecutionMode;
use crate::runtime::orchestrator::AgentRunDeps;
use crate::runtime::types::ToolCallItem;

use super::dispatch::ToolExecutionConsumer;

mod parallel;
mod sequential;
pub mod transcript;

pub(crate) struct ToolExecutionResult {
    pub events: Vec<Event>,
    pub completed_calls: usize,
    pub failed_calls: usize,
}

#[derive(PartialEq)]
enum ExecutionMode {
    Parallel,
    Sequential,
}

fn resolve_execution_mode(
    calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    settings: &ModelRunSettings,
) -> ExecutionMode {
    if settings.parallel_tools == Some(false) {
        return ExecutionMode::Sequential;
    }
    for tc in calls {
        if let Some(route) = routes.get(&tc.name)
            && matches!(
                &route.tool_def.execution_mode,
                Some(ToolExecutionMode::Sequential)
            )
        {
            return ExecutionMode::Sequential;
        }
    }
    ExecutionMode::Parallel
}

pub(crate) async fn execute_tool_calls_with_deps(
    deps: &AgentRunDeps,
    tool_calls: &[ToolCallItem],
    routes: &HashMap<String, CatalogRoute>,
    model_settings: &ModelRunSettings,
    cancel: CancellationToken,
    transcript: &mut TranscriptManager,
    turn_index: u32,
    tool_consumer: &ToolExecutionConsumer,
) -> Result<ToolExecutionResult, String> {
    let mode = resolve_execution_mode(tool_calls, routes, model_settings);
    if mode == ExecutionMode::Parallel {
        parallel::execute_parallel_direct(
            deps,
            tool_calls,
            routes,
            cancel.clone(),
            transcript,
            turn_index,
            tool_consumer,
        )
        .await
    } else {
        sequential::execute_sequential_direct(
            deps,
            tool_calls,
            routes,
            cancel.clone(),
            transcript,
            turn_index,
            tool_consumer,
        )
        .await
    }
}
