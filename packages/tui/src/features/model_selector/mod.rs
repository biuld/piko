use ratatui::{Frame, layout::Rect};

use crate::{
    navigation::SelectBandBudget,
    theme::Theme,
    ui::components::selectable_list::{
        ColumnCell, SelectableItem, SelectableList, render_selectable_list_minimal,
    },
};

/// A discovered model option.
#[derive(Clone, Debug)]
pub struct ModelOption {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub has_auth: bool,
}

impl ModelOption {
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

/// Flat model picker on shared [`SelectableList`] + Columns body.
pub struct ModelSelector {
    pub list: SelectableList<ModelOption>,
    pub filter: String,
}

impl ModelSelector {
    pub fn new() -> Self {
        Self {
            list: SelectableList::new(Vec::new()),
            filter: String::new(),
        }
    }

    pub fn load(&mut self, models: Vec<ModelOption>) {
        self.list = SelectableList::new(models);
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// ComposerBand content-row budget (dense Columns rows).
    pub fn select_band_budget(&self) -> SelectBandBudget {
        SelectBandBudget::minimal_dense_list(self.filtered_count())
    }

    pub fn reset(&mut self) {
        self.list.selected = 0;
    }

    pub fn select_next(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_next(&filter, |m| model_matches(m, &filter));
    }

    pub fn select_prev(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_prev(&filter, |m| model_matches(m, &filter));
    }

    /// Confirm highlighted model (already filtered list).
    pub fn confirm(&mut self) -> Option<ModelOption> {
        let filter = self.filter.as_str();
        let filtered = self
            .list
            .filtered_indices(filter, |m| model_matches(m, filter));
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
            .and_then(|&idx| self.list.items.get(idx).cloned())
    }

    fn filtered_count(&self) -> usize {
        self.list
            .filtered_indices(&self.filter, |m| model_matches(m, &self.filter))
            .len()
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        active_model_id: Option<&str>,
        theme: &Theme,
    ) {
        let items: Vec<SelectableItem> = self
            .list
            .items
            .iter()
            .map(|action| {
                let model_id_full = action.full_id();
                let is_active = active_model_id
                    .map(|id| id == model_id_full)
                    .unwrap_or(false);
                let auth = if action.has_auth { "auth" } else { "no auth" };
                SelectableItem::columns([
                    ColumnCell::primary(model_id_full),
                    ColumnCell::secondary(action.name.clone()),
                    ColumnCell::secondary(auth),
                ])
                .active(is_active)
            })
            .collect();

        render_selectable_list_minimal(
            frame,
            area,
            "models",
            &items,
            self.list.selected,
            &self.filter,
            true,
            theme,
        );
    }
}

fn model_matches(m: &ModelOption, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let f = filter.to_lowercase();
    m.provider.to_lowercase().contains(&f)
        || m.id.to_lowercase().contains(&f)
        || m.name.to_lowercase().contains(&f)
        || m.full_id().to_lowercase().contains(&f)
        || if m.has_auth { "auth" } else { "no auth" }.contains(filter)
}
