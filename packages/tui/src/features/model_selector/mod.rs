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
        active_provider: Option<&str>,
        theme: &Theme,
    ) {
        let items: Vec<SelectableItem> = self
            .list
            .items
            .iter()
            .map(|action| {
                let is_active = model_is_active(action, active_model_id, active_provider);
                let auth = if action.has_auth { "auth" } else { "no auth" };
                SelectableItem::columns([
                    ColumnCell::primary(action.full_id()),
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

/// Whether `model` is the currently active one. Host reports the active model
/// as a bare id (or name); match full `provider/id`, provider-scoped bare id,
/// or model name.
fn model_is_active(
    model: &ModelOption,
    active_model_id: Option<&str>,
    active_provider: Option<&str>,
) -> bool {
    let Some(id) = active_model_id else {
        return false;
    };
    id == model.full_id()
        || (active_provider.is_none_or(|p| p == model.provider.as_str()) && id == model.id)
        || id == model.name
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

#[cfg(test)]
mod tests {
    use super::*;

    fn option(provider: &str, id: &str, name: &str) -> ModelOption {
        ModelOption {
            provider: provider.into(),
            id: id.into(),
            name: name.into(),
            has_auth: true,
        }
    }

    #[test]
    fn active_matches_bare_id_with_provider() {
        let m = option("openai", "gpt-4o", "GPT-4o");
        assert!(model_is_active(&m, Some("gpt-4o"), Some("openai")));
        assert!(!model_is_active(&m, Some("gpt-4o"), Some("anthropic")));
    }

    #[test]
    fn active_matches_full_id_and_name() {
        let m = option("openai", "gpt-4o", "GPT-4o");
        assert!(model_is_active(&m, Some("openai/gpt-4o"), None));
        assert!(model_is_active(&m, Some("GPT-4o"), None));
        assert!(!model_is_active(&m, Some("claude-3"), Some("openai")));
    }
}
