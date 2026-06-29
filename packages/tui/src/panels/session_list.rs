use piko_protocol::SessionSummary;
use ratatui::{Frame, layout::Rect};

use crate::{
    components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
    panels::short_id,
};

/// Session list panel.
pub struct SessionList {
    pub list: FilterableList<SessionSummary>,
}

impl SessionList {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
        }
    }

    pub fn load(&mut self, sessions: Vec<SessionSummary>) {
        self.list = FilterableList::new(sessions);
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.session_id.to_lowercase().contains(filter)
                || item.cwd.to_lowercase().contains(filter)
                || item
                    .name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(filter))
                    .unwrap_or(false)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.session_id.to_lowercase().contains(filter)
                || item.cwd.to_lowercase().contains(filter)
                || item
                    .name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(filter))
                    .unwrap_or(false)
        });
    }

    pub fn selected_session_id(&self, filter: &str) -> Option<String> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.session_id.to_lowercase().contains(filter)
                || item.cwd.to_lowercase().contains(filter)
                || item
                    .name
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(filter))
                    .unwrap_or(false)
        });
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
            .map(|item| item.session_id.clone())
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        active_session_id: Option<&str>,
    ) {
        let items: Vec<FilterableItem> = self
            .list
            .items
            .iter()
            .map(|item| {
                let name = item.name.as_deref().unwrap_or("untitled");
                let id = short_id(&item.session_id);
                let is_active = active_session_id
                    .map(|id| id == item.session_id)
                    .unwrap_or(false);
                FilterableItem {
                    primary: format!("{}  {}", name, id),
                    detail: format!("{}  seq {}", item.cwd, item.seq),
                    is_active,
                }
            })
            .collect();
        render_filterable_list(frame, area, "sessions", &items, self.list.selected, filter);
    }
}
