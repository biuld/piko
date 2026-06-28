// ---- Application: tools — provider/toolset/model-config wiring ----

use crate::domain::model::step::ModelConfig;
use crate::domain::tools::definition::ToolSet;
use crate::ports::approval_gateway::ApprovalGateway;
use crate::ports::tool_provider::ToolProvider;

use super::orchestrator::OrchCore;
use piko_protocol::runtime::OrchModelConfig;

/// Register a tool provider.
pub async fn register_provider(core: &OrchCore, provider: Box<dyn ToolProvider>) {
    core.tool_registry.register_provider(provider).await;
}

/// Register a tool set.
pub async fn register_tool_set(core: &OrchCore, tool_set: ToolSet) {
    core.tool_registry.register_tool_set(tool_set).await;
}

/// Unregister a tool set.
pub async fn unregister_tool_set(core: &OrchCore, tool_set_id: String) {
    core.tool_registry.unregister_tool_set(&tool_set_id).await;
}

/// Set the approval gateway.
pub async fn set_approval_gateway(core: &OrchCore, gateway: Option<Box<dyn ApprovalGateway>>) {
    core.tool_registry.set_approval_gateway(gateway).await;
}

/// Set model config used by subsequently created agent runs.
pub async fn set_model_config(core: &OrchCore, config: OrchModelConfig) {
    let model_config = ModelConfig {
        model: crate::domain::model::step::ModelSpec {
            id: config.model.id.clone(),
            name: config.model.name.clone(),
            provider: config.model.provider.clone(),
        },
        provider: config.provider,
        settings: config.settings,
        thinking_level_map: config.thinking_level_map,
    };

    {
        let mut mc = core.latest_model_config.write().await;
        *mc = Some(model_config.clone());
    }
}
