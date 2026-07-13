use std::collections::HashMap;

use crate::adapters::tools::registry::CatalogRoute;
use crate::domain::model::step::ModelRunSettings;
use crate::domain::tools::call::ToolCallItem;
use crate::domain::tools::definition::ToolExecutionMode;

pub(crate) mod executor;
pub mod transcript;

pub(crate) use executor::{SharedToolCallCollector, ToolCallDispatchConsumer};
pub(crate) use transcript::{build_tool_error, build_tool_result};

#[derive(PartialEq)]
enum ExecutionMode {
    Parallel,
    Sequential,
}

#[allow(dead_code)]
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
