use super::{HistoryPanel, HistoryRow};
use std::ops::Range;

impl HistoryPanel {
    pub fn visible_rows(&self) -> Vec<HistoryRow> {
        self.rows_matching(&self.filter)
    }

    pub(super) fn visible_rows_range(&self, range: Range<usize>) -> Vec<HistoryRow> {
        let count = range.end.saturating_sub(range.start);
        if self.choosing_session {
            return self
                .sessions
                .iter()
                .filter(|session| {
                    matches_text(&self.filter, &session.session_id)
                        || matches_text(&self.filter, session.name.as_deref().unwrap_or(""))
                        || matches_text(&self.filter, &session.cwd)
                })
                .skip(range.start)
                .take(count)
                .cloned()
                .map(HistoryRow::Session)
                .collect();
        }
        if self.agent_choosing {
            let Some(overview) = &self.overview else {
                return Vec::new();
            };
            return nested_agents(&overview.agents)
                .into_iter()
                .filter(|(_, agent)| {
                    matches_text(&self.filter, &agent.agent_instance_id)
                        || matches_text(&self.filter, &agent.agent_spec_id)
                })
                .skip(range.start)
                .take(count)
                .map(|(depth, agent)| HistoryRow::Agent {
                    agent: agent.clone(),
                    depth,
                })
                .collect();
        }
        self.stream
            .as_ref()
            .into_iter()
            .flat_map(|stream| stream.items.iter())
            .filter(|item| stream_item_matches(&self.filter, item))
            .skip(range.start)
            .take(count)
            .cloned()
            .map(HistoryRow::Stream)
            .collect()
    }

    pub(super) fn loaded_row_count(&self) -> usize {
        if self.choosing_session {
            return self.sessions.len();
        }
        if self.agent_choosing {
            return self
                .overview
                .as_ref()
                .map_or(0, |overview| overview.agents.len());
        }
        self.stream.as_ref().map_or(0, |stream| stream.items.len())
    }

    pub(super) fn visible_row_count(&self) -> usize {
        if self.choosing_session {
            return self
                .sessions
                .iter()
                .filter(|session| {
                    matches_text(&self.filter, &session.session_id)
                        || matches_text(&self.filter, session.name.as_deref().unwrap_or(""))
                        || matches_text(&self.filter, &session.cwd)
                })
                .count();
        }
        if self.agent_choosing {
            let Some(overview) = &self.overview else {
                return 0;
            };
            return overview
                .agents
                .iter()
                .filter(|agent| {
                    matches_text(&self.filter, &agent.agent_instance_id)
                        || matches_text(&self.filter, &agent.agent_spec_id)
                })
                .count();
        }
        self.stream.as_ref().map_or(0, |stream| {
            stream
                .items
                .iter()
                .filter(|item| stream_item_matches(&self.filter, item))
                .count()
        })
    }

    pub(super) fn stream_item_at(&self, index: usize) -> Option<&piko_protocol::HistoryStreamItem> {
        if self.choosing_session || self.agent_choosing {
            return None;
        }
        self.stream
            .as_ref()?
            .items
            .iter()
            .filter(|item| stream_item_matches(&self.filter, item))
            .nth(index)
    }

    fn rows_matching(&self, filter: &str) -> Vec<HistoryRow> {
        if self.choosing_session {
            return self
                .sessions
                .iter()
                .filter(|session| {
                    matches_text(filter, &session.session_id)
                        || matches_text(filter, session.name.as_deref().unwrap_or(""))
                        || matches_text(filter, &session.cwd)
                })
                .cloned()
                .map(HistoryRow::Session)
                .collect();
        }
        if self.agent_choosing {
            let Some(overview) = &self.overview else {
                return Vec::new();
            };
            return nested_agents(&overview.agents)
                .into_iter()
                .filter(|(_, agent)| {
                    matches_text(filter, &agent.agent_instance_id)
                        || matches_text(filter, &agent.agent_spec_id)
                })
                .map(|(depth, agent)| HistoryRow::Agent {
                    agent: agent.clone(),
                    depth,
                })
                .collect();
        }
        let Some(stream) = &self.stream else {
            return Vec::new();
        };
        stream
            .items
            .iter()
            .filter(|item| stream_item_matches(filter, item))
            .cloned()
            .map(HistoryRow::Stream)
            .collect()
    }

    pub fn selected_agent_id(&self) -> Option<String> {
        match self.visible_rows().get(self.selected)? {
            HistoryRow::Agent { agent, .. } => Some(agent.agent_instance_id.clone()),
            _ => None,
        }
    }
}

fn nested_agents(
    agents: &[piko_protocol::HistoryAgentSummary],
) -> Vec<(u32, &piko_protocol::HistoryAgentSummary)> {
    let ids = agents
        .iter()
        .map(|agent| agent.agent_instance_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut by_parent: std::collections::BTreeMap<
        Option<String>,
        Vec<&piko_protocol::HistoryAgentSummary>,
    > = std::collections::BTreeMap::new();
    for agent in agents {
        let parent = agent
            .parent_agent_instance_id
            .clone()
            .filter(|id| ids.contains(id.as_str()));
        by_parent.entry(parent).or_default().push(agent);
    }
    let mut rows = Vec::new();
    walk_agents(&by_parent, None, 0, &mut rows);
    rows
}

fn walk_agents<'a>(
    by_parent: &std::collections::BTreeMap<
        Option<String>,
        Vec<&'a piko_protocol::HistoryAgentSummary>,
    >,
    parent: Option<&str>,
    depth: u32,
    rows: &mut Vec<(u32, &'a piko_protocol::HistoryAgentSummary)>,
) {
    let Some(children) = by_parent.get(&parent.map(str::to_string)) else {
        return;
    };
    for agent in children {
        rows.push((depth, agent));
        walk_agents(
            by_parent,
            Some(agent.agent_instance_id.as_str()),
            depth + 1,
            rows,
        );
    }
}

fn matches_text(filter: &str, value: &str) -> bool {
    filter.is_empty()
        || value
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
}

fn stream_item_matches(filter: &str, item: &piko_protocol::HistoryStreamItem) -> bool {
    matches_text(filter, &item.summary)
        || item
            .relation
            .root_input_id
            .as_deref()
            .is_some_and(|id| matches_text(filter, id))
}

/// Default agent for a freshly inspected session: the first root agent, or
/// the first listed agent.
pub fn default_agent_id(agents: &[piko_protocol::HistoryAgentSummary]) -> Option<String> {
    agents
        .iter()
        .find(|agent| agent.parent_agent_instance_id.is_none())
        .or_else(|| agents.first())
        .map(|agent| agent.agent_instance_id.clone())
}
