use piko_protocol::agents::AgentSpec;

pub(super) fn root_agent_spec(cwd: impl AsRef<std::path::Path>) -> AgentSpec {
    let mut spec = crate::adapters::prompts::agent_loader::load_agents(cwd)
        .remove("main")
        .expect("built-in main agent must be registered");
    ensure_root_tool_sets(&mut spec);
    spec
}

/// Ensure root mandatory packs when a custom project agent omits them.
///
/// TOML remains the source of truth; this only adds missing
/// `user_interaction` / `multi_agent` for the live root turn.
pub(super) fn ensure_root_tool_sets(spec: &mut AgentSpec) {
    if !spec.tool_set_ids.iter().any(|id| id == "user_interaction") {
        spec.tool_set_ids.push("user_interaction".into());
    }
    if !spec.tool_set_ids.iter().any(|id| id == "multi_agent") {
        spec.tool_set_ids.push("multi_agent".into());
    }
}

/// Resolve the AgentSpec used while attaching a recovered AgentInstance.
///
/// Recovery always prefers the durable immutable AgentSpec snapshot. Registry
/// definitions are fallbacks only for legacy entries without a stored spec.
pub(super) fn resolve_recovered_agent_spec(
    agent_instance_id: &str,
    root_agent_instance_id: &str,
    stored_spec: Option<AgentSpec>,
    recovered_spec_id: &str,
    resolved_specs: &std::collections::HashMap<String, AgentSpec>,
    root_agent_spec: &AgentSpec,
) -> AgentSpec {
    stored_spec.unwrap_or_else(|| {
        if agent_instance_id == root_agent_instance_id {
            return root_agent_spec.clone();
        }
        resolved_specs
            .get(recovered_spec_id)
            .cloned()
            .or_else(|| {
                resolved_specs
                    .values()
                    .find(|spec| spec.id == recovered_spec_id)
                    .cloned()
            })
            .unwrap_or_else(|| root_agent_spec.clone())
    })
}
