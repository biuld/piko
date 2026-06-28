// ---- Orchestrator: tool — provider/toolset/model-config wiring ----

use crate::protocol::runtime::OrchModelConfig;
use crate::protocol::tools::ToolSet;
use crate::tools::{ApprovalGateway, ToolProvider};

use super::core::OrchCore;
use crate::actors::agent::types::ModelConfig;
use tokio_actors::ActorSystem;

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

/// Set model config: store it and broadcast to all agent actors.
pub async fn set_model_config(core: &OrchCore, config: OrchModelConfig) {
    let model_config = ModelConfig {
        model: crate::actors::agent::types::ModelSpec {
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

    // Notify all registered agent actors about the new model config
    let specs = core.agent_specs.read().await;
    for (agent_id, _spec) in specs.iter() {
        let msg = crate::actors::agent::types::AgentMsg::SetModelConfig {
            config: model_config.clone(),
        };
        if let Some(handle) =
            ActorSystem::default().get::<crate::actors::agent::actor::AgentActor>(agent_id)
        {
            let _ = handle.notify(msg).await;
        }
    }
}
