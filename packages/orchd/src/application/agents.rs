// ---- Application: agents — register/unregister agent actors ----

use std::sync::Arc;

use tokio_actors::ActorSystem;
use tokio_actors::actor::ActorExt;

use crate::domain::agents::spec::AgentSpec;
use crate::runtime::agent_actor::actor::AgentActor;
use crate::runtime::agent_actor::messages::AgentActorDeps;
use crate::application::orchestrator::OrchCore;

/// Register an agent specification: spawns an AgentActor via tokio_actors.
pub async fn register_agent(core: &OrchCore, spec: AgentSpec) {
    let event_tx = Arc::clone(&core.event_tx);

    let model_config = if let Some(model_id) = &spec.model {
        // Agent specifies a model — resolve it from the global provider config
        let mut base = core.latest_model_config.read().await.clone();
        if let Some(ref mut mc) = base {
            mc.model.id = model_id.clone();
            mc.model.name = model_id.clone();
        }
        // Apply thinking_level override if specified
        if let Some(ref mut mc) = base {
            if let Some(ref thinking) = spec.thinking_level {
                mc.settings.thinking_level = Some(thinking.clone());
            }
        }
        base
    } else {
        let mut base = core.latest_model_config.read().await.clone();
        // Apply thinking_level override on inherited config
        if let Some(ref mut mc) = base {
            if let Some(ref thinking) = spec.thinking_level {
                mc.settings.thinking_level = Some(thinking.clone());
            }
        }
        base
    };

    let deps = AgentActorDeps {
        model_executor: Arc::clone(&core.model_executor),
        model_config,
        tool_registry: Arc::clone(&core.tool_registry),
        event_tx,
    };

    let actor = AgentActor::new(spec.clone(), deps);
    let _handle = actor
        .spawn()
        .named(&spec.id)
        .await
        .expect("Failed to spawn agent actor");

    let mut specs = core.agent_specs.write().await;
    specs.insert(spec.id.clone(), spec);
}

/// Unregister an agent: stops its actor via tokio_actors.
pub async fn unregister_agent(core: &OrchCore, agent_id: String) {
    if let Some(handle) = ActorSystem::default().get::<AgentActor>(&agent_id) {
        let _ = handle.stop(tokio_actors::StopReason::Graceful).await;
    }
    let mut specs = core.agent_specs.write().await;
    specs.remove(&agent_id);
}
