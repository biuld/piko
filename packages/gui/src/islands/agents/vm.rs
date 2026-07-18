//! Agent tree projection for selection.

use piko_client_core::ClientState;
use piko_protocol::AgentActivity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTreeNode {
    pub agent_instance_id: String,
    pub parent_agent_instance_id: Option<String>,
    pub name: String,
    pub role: String,
    pub depth: usize,
    pub selected: bool,
    pub has_children: bool,
    pub activity_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentTreeViewModel {
    pub nodes: Vec<AgentTreeNode>,
}

pub fn derive_agent_tree(state: &ClientState) -> AgentTreeViewModel {
    let Some(session) = state.live_session.as_ref() else {
        return AgentTreeViewModel::default();
    };

    let selected = session.selected_agent.as_deref();
    let mut nodes = Vec::new();

    // Roots first, then children by parent walk (stable host order preserved within levels).
    fn walk(
        agents: &[piko_protocol::AgentInfo],
        parent: Option<&str>,
        depth: usize,
        selected: Option<&str>,
        out: &mut Vec<AgentTreeNode>,
    ) {
        for agent in agents
            .iter()
            .filter(|a| a.parent_agent_instance_id.as_deref() == parent)
        {
            let has_children = agents.iter().any(|candidate| {
                candidate.parent_agent_instance_id.as_deref()
                    == Some(agent.agent_instance_id.as_str())
            });
            out.push(AgentTreeNode {
                agent_instance_id: agent.agent_instance_id.clone(),
                parent_agent_instance_id: agent.parent_agent_instance_id.clone(),
                name: agent.name.clone(),
                role: agent.role.clone(),
                depth,
                selected: selected == Some(agent.agent_instance_id.as_str()),
                has_children,
                activity_label: activity_label(&agent.activity),
            });
            walk(
                agents,
                Some(agent.agent_instance_id.as_str()),
                depth + 1,
                selected,
                out,
            );
        }
    }

    walk(&session.agents, None, 0, selected, &mut nodes);
    AgentTreeViewModel { nodes }
}

/// Whether `node` should appear given collapsed ancestors.
pub(crate) fn agent_node_visible(
    node: &AgentTreeNode,
    nodes: &[AgentTreeNode],
    collapsed: &std::collections::HashSet<String>,
) -> bool {
    let mut parent = node.parent_agent_instance_id.as_deref();
    while let Some(pid) = parent {
        if collapsed.contains(pid) {
            return false;
        }
        parent = nodes
            .iter()
            .find(|n| n.agent_instance_id == pid)
            .and_then(|n| n.parent_agent_instance_id.as_deref());
    }
    true
}

fn activity_label(activity: &AgentActivity) -> String {
    match activity {
        AgentActivity::Idle => "idle".into(),
        AgentActivity::Running => "running".into(),
        AgentActivity::WaitingForApproval => "approval".into(),
        AgentActivity::Cancelling => "cancelling".into(),
    }
}
