use ratatui::{Frame, layout::Rect};

use crate::{
    components::filterable_list::{FilterableItem, FilterableList, render_filterable_list},
    theme::Theme,
};

/// A discovered model option.
#[derive(Clone)]
pub struct ModelOption {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub has_auth: bool,
}

/// Model selector panel.
pub struct ModelSelector {
    pub list: FilterableList<ModelOption>,
}

impl ModelSelector {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
        }
    }

    pub fn load(&mut self, models: Vec<ModelOption>) {
        self.list = FilterableList::new(models);
    }

    pub fn select_next(&mut self, filter: &str) {
        self.list.select_next(filter, |item| {
            item.id.to_lowercase().contains(filter)
                || item.provider.to_lowercase().contains(filter)
                || item.name.to_lowercase().contains(filter)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        self.list.select_prev(filter, |item| {
            item.id.to_lowercase().contains(filter)
                || item.provider.to_lowercase().contains(filter)
                || item.name.to_lowercase().contains(filter)
        });
    }

    pub fn selected_model(&self, filter: &str) -> Option<&ModelOption> {
        let filtered = self.list.filtered_indices(filter, |item| {
            item.id.to_lowercase().contains(filter)
                || item.provider.to_lowercase().contains(filter)
                || item.name.to_lowercase().contains(filter)
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
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        active_model_id: Option<&str>,
        theme: &Theme,
    ) {
        let items: Vec<FilterableItem> = self
            .list
            .items
            .iter()
            .map(|item| {
                let auth = if item.has_auth { "auth" } else { "no auth" };
                let model_id_full = format!("{}/{}", item.provider, item.id);
                let is_active = active_model_id
                    .map(|id| id == model_id_full)
                    .unwrap_or(false);
                FilterableItem {
                    primary: model_id_full,
                    detail: format!("{}  {auth}", item.name),
                    is_active,
                }
            })
            .collect();
        render_filterable_list(
            frame,
            area,
            "models",
            &items,
            self.list.selected,
            filter,
            theme,
        );
    }
}
