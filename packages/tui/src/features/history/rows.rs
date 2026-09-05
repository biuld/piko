use piko_protocol::{
    HistoryAgentSummary, HistoryItemSummary, HistoryProvenance, HistoryProvenanceFilter,
};

use super::{HistoryLens, HistoryPanel, HistoryRow};

impl HistoryPanel {
    pub fn visible_rows(&self) -> Vec<HistoryRow> {
        self.rows_matching(&self.filter)
    }

    pub(super) fn loaded_row_count(&self) -> usize {
        self.rows_matching("").len()
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
        match self.lens {
            HistoryLens::Work | HistoryLens::Agents if self.work.is_some() => {
                self.work_item_rows(filter)
            }
            HistoryLens::Work | HistoryLens::Agents if self.agent_id.is_some() => {
                self.agent_work_rows(filter)
            }
            HistoryLens::Work => self.work_summary_rows(filter),
            HistoryLens::Agents => self.agent_rows(filter),
            HistoryLens::Transcript => self.transcript_rows(filter),
            HistoryLens::Journal => self.journal_rows(filter),
        }
    }

    fn work_summary_rows(&self, filter: &str) -> Vec<HistoryRow> {
        self.overview
            .as_ref()
            .map(|overview| {
                overview
                    .works
                    .iter()
                    .filter(|work| {
                        matches_text(filter, &work.input_preview)
                            || matches_text(filter, &work.root_input_id)
                    })
                    .cloned()
                    .map(HistoryRow::Work)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn agent_work_rows(&self, filter: &str) -> Vec<HistoryRow> {
        let Some(agent_id) = &self.agent_id else {
            return Vec::new();
        };
        self.overview
            .as_ref()
            .map(|overview| {
                overview
                    .works
                    .iter()
                    .filter(|work| work.agent_instance_id == *agent_id)
                    .filter(|work| {
                        matches_text(filter, &work.input_preview)
                            || matches_text(filter, &work.root_input_id)
                    })
                    .cloned()
                    .map(HistoryRow::Work)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn agent_rows(&self, filter: &str) -> Vec<HistoryRow> {
        let Some(overview) = &self.overview else {
            return Vec::new();
        };
        nested_agents(&overview.agents)
            .into_iter()
            .filter(|(_, agent)| {
                matches_text(filter, &agent.agent_instance_id)
                    || matches_text(filter, &agent.agent_spec_id)
            })
            .map(|(depth, agent)| HistoryRow::Agent {
                agent: agent.clone(),
                depth,
            })
            .collect()
    }

    fn work_item_rows(&self, filter: &str) -> Vec<HistoryRow> {
        let Some(page) = &self.work else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for item in &page.items {
            self.push_item_rows(&mut rows, item, 0, filter);
        }
        rows
    }

    fn push_item_rows(
        &self,
        rows: &mut Vec<HistoryRow>,
        item: &HistoryItemSummary,
        depth: u32,
        filter: &str,
    ) {
        let show_self = match self.provenance {
            HistoryProvenanceFilter::All => true,
            HistoryProvenanceFilter::Facts => item.provenance == HistoryProvenance::Fact,
            HistoryProvenanceFilter::Diagnostics => {
                item.provenance == HistoryProvenance::Diagnostic
            }
        };
        if show_self && matches_text(filter, &item.summary) {
            rows.push(HistoryRow::Item {
                item: item.clone(),
                depth,
            });
        }
        if self.provenance != HistoryProvenanceFilter::Facts {
            for child in &item.children {
                self.push_item_rows(rows, child, depth + 1, filter);
            }
        }
    }

    fn transcript_rows(&self, filter: &str) -> Vec<HistoryRow> {
        self.transcript
            .as_ref()
            .map(|page| {
                page.items
                    .iter()
                    .filter(|item| matches_text(filter, &item.summary))
                    .cloned()
                    .map(HistoryRow::Transcript)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn journal_rows(&self, filter: &str) -> Vec<HistoryRow> {
        let Some(page) = &self.journal else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for commit in &page.commits {
            let events = commit
                .events
                .iter()
                .filter(|item| match self.provenance {
                    HistoryProvenanceFilter::All => true,
                    HistoryProvenanceFilter::Facts => item.provenance == HistoryProvenance::Fact,
                    HistoryProvenanceFilter::Diagnostics => {
                        item.provenance == HistoryProvenance::Diagnostic
                    }
                })
                .filter(|item| matches_text(filter, &item.summary))
                .cloned()
                .collect::<Vec<_>>();
            if events.is_empty() {
                continue;
            }
            rows.push(HistoryRow::CommitHeader {
                revision: commit.revision,
                producer: commit.producer.clone(),
                events: events.len(),
                committed_at: commit.committed_at,
            });
            rows.extend(
                events
                    .into_iter()
                    .map(|item| HistoryRow::Item { item, depth: 1 }),
            );
        }
        rows
    }
}

fn nested_agents(agents: &[HistoryAgentSummary]) -> Vec<(u32, &HistoryAgentSummary)> {
    let ids = agents
        .iter()
        .map(|agent| agent.agent_instance_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut by_parent: std::collections::BTreeMap<Option<String>, Vec<&HistoryAgentSummary>> =
        std::collections::BTreeMap::new();
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
    by_parent: &std::collections::BTreeMap<Option<String>, Vec<&'a HistoryAgentSummary>>,
    parent: Option<&str>,
    depth: u32,
    rows: &mut Vec<(u32, &'a HistoryAgentSummary)>,
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
