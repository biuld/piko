//! Agent tree projection for selection.

use piko_client_core::{ClientState, agent_foreground};
use piko_protocol::AgentForeground;

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
        session: &piko_client_core::LiveSession,
        parent: Option<&str>,
        depth: usize,
        selected: Option<&str>,
        out: &mut Vec<AgentTreeNode>,
    ) {
        let agents = &session.agents;
        for agent in agents
            .iter()
            .filter(|a| a.parent_agent_instance_id.as_deref() == parent)
        {
            let has_children = agents.iter().any(|candidate| {
                candidate.parent_agent_instance_id.as_deref()
                    == Some(agent.agent_instance_id.as_str())
            });
            let fg = agent_foreground(
                &agent.agent_instance_id,
                &session.agents,
                &session.active_turns,
                &session.pending_approvals,
                &session.pending_interactions,
            );
            out.push(AgentTreeNode {
                agent_instance_id: agent.agent_instance_id.clone(),
                parent_agent_instance_id: agent.parent_agent_instance_id.clone(),
                name: agent.name.clone(),
                role: agent.role.clone(),
                depth,
                selected: selected == Some(agent.agent_instance_id.as_str()),
                has_children,
                activity_label: foreground_label(fg),
            });
            walk(
                session,
                Some(agent.agent_instance_id.as_str()),
                depth + 1,
                selected,
                out,
            );
        }
    }

    walk(session, None, 0, selected, &mut nodes);
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

fn foreground_label(foreground: AgentForeground) -> String {
    match foreground {
        AgentForeground::Idle => crate::t!("agent.activity.idle"),
        AgentForeground::Running => crate::t!("agent.activity.running"),
        AgentForeground::RequiresAction => crate::t!("agent.activity.approval"),
        AgentForeground::Cancelling => crate::t!("agent.activity.cancelling"),
        AgentForeground::Queued => crate::t!("agent.activity.queued"),
    }
}
