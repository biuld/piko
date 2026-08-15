use std::collections::HashMap;

use piko_protocol::SessionTreeEntry;

use super::Timeline;

/// Owns every timeline projection for the open session.
///
/// The active projection is kept separate for cheap rendering, but switching,
/// session-wide entry fan-out, and inactive-agent initialization are atomic
/// operations of this store rather than coordination performed by `AppState`.
pub struct TimelineStore {
    active: Timeline,
    inactive: HashMap<String, Timeline>,
    session_entries: Vec<(SessionTreeEntry, u64)>,
}

impl TimelineStore {
    pub fn new() -> Self {
        Self {
            active: Timeline::new(),
            inactive: HashMap::new(),
            session_entries: Vec::new(),
        }
    }

    pub fn active(&self) -> &Timeline {
        &self.active
    }

    pub fn active_mut(&mut self) -> &mut Timeline {
        &mut self.active
    }

    #[cfg(test)]
    pub fn inactive(&self, agent_instance_id: &str) -> Option<&Timeline> {
        self.inactive.get(agent_instance_id)
    }

    pub fn session_entries(&self) -> &[(SessionTreeEntry, u64)] {
        &self.session_entries
    }

    pub fn clear(&mut self) {
        self.active.clear();
        self.inactive.clear();
        self.session_entries.clear();
    }

    pub fn begin_projection_batch(&mut self) {
        self.active.begin_projection_batch();
        for timeline in self.inactive.values_mut() {
            timeline.begin_projection_batch();
        }
    }

    pub fn end_projection_batch(&mut self) {
        self.active.end_projection_batch();
        for timeline in self.inactive.values_mut() {
            timeline.end_projection_batch();
        }
    }

    pub fn ensure_inactive(&mut self, agent_instance_id: impl Into<String>) {
        let agent_instance_id = agent_instance_id.into();
        if !self.inactive.contains_key(&agent_instance_id) {
            let timeline =
                Self::seeded_timeline(&self.session_entries, self.active.thinking_visible);
            self.inactive.insert(agent_instance_id, timeline);
        }
    }

    pub fn set_thinking_visible(&mut self, visible: bool) {
        self.active.thinking_visible = visible;
        for timeline in self.inactive.values_mut() {
            timeline.thinking_visible = visible;
        }
    }

    pub fn with_agent(
        &mut self,
        active_agent_instance_id: Option<&str>,
        agent_instance_id: &str,
        apply: impl FnOnce(&mut Timeline),
    ) {
        if active_agent_instance_id.is_none_or(|active| active == agent_instance_id) {
            apply(&mut self.active);
            return;
        }
        self.ensure_inactive(agent_instance_id.to_string());
        apply(
            self.inactive
                .get_mut(agent_instance_id)
                .expect("inactive timeline was initialized"),
        );
    }

    pub fn select(&mut self, previous: Option<&str>, agent_instance_id: &str) {
        if previous == Some(agent_instance_id) {
            return;
        }
        let next = self.inactive.remove(agent_instance_id).unwrap_or_else(|| {
            Self::seeded_timeline(&self.session_entries, self.active.thinking_visible)
        });
        let previous_timeline = std::mem::replace(&mut self.active, next);
        if let Some(previous) = previous {
            self.inactive
                .insert(previous.to_string(), previous_timeline);
        }
    }

    pub fn apply_session_entry(
        &mut self,
        entry: SessionTreeEntry,
        order: u64,
    ) -> piko_client_core::ApplyOutcome {
        let mut outcome = self.active.apply_session_entry(entry.clone(), order);
        for timeline in self.inactive.values_mut() {
            let next = timeline.apply_session_entry(entry.clone(), order);
            if next == piko_client_core::ApplyOutcome::Inconsistent {
                outcome = next;
            }
        }
        if outcome != piko_client_core::ApplyOutcome::Ignored
            && self
                .session_entries
                .iter()
                .all(|(existing, _)| existing.id() != entry.id())
        {
            self.session_entries.push((entry, order));
        }
        outcome
    }

    fn seeded_timeline(entries: &[(SessionTreeEntry, u64)], thinking_visible: bool) -> Timeline {
        let mut timeline = Timeline::new();
        timeline.thinking_visible = thinking_visible;
        for (entry, order) in entries {
            let _ = timeline.apply_session_entry(entry.clone(), *order);
        }
        timeline
    }
}

impl Default for TimelineStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_policy_applies_to_existing_and_future_agent_views() {
        let mut store = TimelineStore::new();
        store.ensure_inactive("agent-a");
        store.set_thinking_visible(false);
        store.ensure_inactive("agent-b");

        assert!(!store.active().thinking_visible);
        assert!(!store.inactive("agent-a").unwrap().thinking_visible);
        assert!(!store.inactive("agent-b").unwrap().thinking_visible);

        store.select(Some("root"), "agent-a");
        assert!(!store.active().thinking_visible);
    }
}
