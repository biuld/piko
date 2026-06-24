// ---- Orchestrator: agent — register/unregister agent actors ----

use std::sync::Arc;

use tokio_actors::ActorSystem;
use tokio_actors::actor::ActorExt;

use crate::actors::agent::actor::AgentActor;
use crate::actors::agent::types::AgentActorDeps;
use crate::orchestrator::core::OrchCore;
use crate::protocol::agents::AgentSpec;

/// Register an agent specification: spawns an AgentActor via tokio_actors.
pub async fn register_agent(core: &OrchCore, spec: AgentSpec) {
    let listeners = Arc::clone(&core.listeners);

    // Build emit function
    let emit_fn: Arc<
        dyn Fn(
                String,
                serde_json::Value,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
            + Send
            + Sync,
    > = Arc::new(move |_agent_id: String, val: serde_json::Value| {
        let listeners = Arc::clone(&listeners);
        Box::pin(async move {
            let ls = listeners.read().await;
            for listener in ls.values() {
                listener(val.clone());
            }
        })
    });

    let model_config = core.latest_model_config.read().await.clone();

    let deps = AgentActorDeps {
        model_executor: Arc::clone(&core.model_executor),
        model_config,
        tool_registry: Arc::clone(&core.tool_registry),
        emit_fn,
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
