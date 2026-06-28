// ---- Application: agents — register/unregister agent specs ----

use crate::application::orchestrator::OrchCore;
use crate::domain::agents::spec::AgentSpec;

/// Register an agent specification.
pub async fn register_agent(core: &OrchCore, spec: AgentSpec) {
    let mut specs = core.agent_specs.write().await;
    specs.insert(spec.id.clone(), spec);
}

/// Unregister an agent.
pub async fn unregister_agent(core: &OrchCore, agent_id: String) {
    let mut specs = core.agent_specs.write().await;
    specs.remove(&agent_id);
}
