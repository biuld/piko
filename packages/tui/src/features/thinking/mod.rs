//! Thinking level selector — Select surface (`SurfaceId::Thinking`,
//! ComposerBand).
//!
//! Band-mode picker for the default reasoning/thinking level, mirroring the
//! Settings catalog "Level" row. Options and copy are shared with the catalog;
//! applying sends the same `default-thinking-level` config patch.

use ratatui::{Frame, layout::Rect};

use piko_tui_layout::{Component, InteractionState, SurfacePanel};

use crate::{
    app::{HitId, command::SurfaceAction},
    navigation::{SelectBandBudget, SurfaceId},
    theme::Theme,
    ui::components::selectable_list::{
        ColumnCell, SelectableItem, SelectableList, minimal_row_regions, paint_row_hover,
        render_selectable_list_minimal,
    },
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture},
};

/// Render context for the thinking-level surface.
pub struct ThinkingCtx<'a> {
    pub active_level: Option<&'a str>,
    pub theme: &'a Theme,
}

impl Component<HitId, ThinkingCtx<'_>> for ThinkingSelector {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &ThinkingCtx<'_>) {
        self.render(frame, area, ctx.active_level, ctx.theme);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &ThinkingCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx.active_level, ctx.theme);
        let items = self.display_items(ctx.active_level);
        let regions =
            minimal_row_regions(area, "thinking", &items, self.list.selected, &self.filter);
        paint_row_hover(frame, &regions, interaction, self.list.selected, ctx.theme);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let items = self.display_items(None);
        minimal_row_regions(area, "thinking", &items, self.list.selected, &self.filter)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect()
    }
}

impl PointerComponent<HitId> for ThinkingSelector {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) if i < self.list.len() => {
                self.list.selected = i;
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::ScrollUp, _) => {
                self.select_prev();
                Vec::new()
            }
            (PointerGesture::ScrollDown, _) => {
                self.select_next();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

impl SurfacePanel<SurfaceId, HitId, ThinkingCtx<'_>> for ThinkingSelector {
    fn region(&self) -> SurfaceId {
        SurfaceId::Thinking
    }
}

use super::settings::thinking_level_detail;

/// Thinking levels, ordered by reasoning budget.
pub const THINKING_LEVELS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh"];

#[derive(Clone, Debug)]
pub struct ThinkingOption {
    level: &'static str,
    detail: &'static str,
}

/// Band-mode thinking picker on shared [`SelectableList`] columns rows.
pub struct ThinkingSelector {
    pub list: SelectableList<ThinkingOption>,
    pub filter: String,
}

impl ThinkingSelector {
    fn display_items(&self, active_level: Option<&str>) -> Vec<SelectableItem> {
        self.list
            .items
            .iter()
            .map(|option| {
                SelectableItem::columns([
                    ColumnCell::primary(option.level),
                    ColumnCell::secondary(option.detail),
                ])
                .active(active_level == Some(option.level))
            })
            .collect()
    }
    pub fn new() -> Self {
        Self {
            list: SelectableList::new(Vec::new()),
            filter: String::new(),
        }
    }

    /// Rebuild the option list around the current active level (if any).
    pub fn prepare(&mut self, active: Option<&str>) {
        self.filter.clear();
        self.list = SelectableList::new(
            THINKING_LEVELS
                .iter()
                .map(|level| ThinkingOption {
                    level,
                    detail: thinking_level_detail(level),
                })
                .collect(),
        );
        if let Some(idx) = THINKING_LEVELS.iter().position(|l| active == Some(l)) {
            self.list.selected = idx;
        }
    }

    /// ComposerBand content-row budget (dense single-line rows).
    pub fn select_band_budget(&self) -> SelectBandBudget {
        SelectBandBudget::minimal_dense_list(self.filtered_count())
    }

    pub fn select_next(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_next(&filter, |o| level_matches(o, &filter));
    }

    pub fn select_prev(&mut self) {
        let filter = self.filter.clone();
        self.list
            .select_prev(&filter, |o| level_matches(o, &filter));
    }

    /// Confirm the highlighted level (already filtered list).
    pub fn confirm(&self) -> Option<&'static str> {
        let filter = self.filter.as_str();
        let filtered = self
            .list
            .filtered_indices(filter, |o| level_matches(o, filter));
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
            .and_then(|&idx| self.list.items.get(idx))
            .map(|o| o.level)
    }

    fn filtered_count(&self) -> usize {
        self.list
            .filtered_indices(&self.filter, |o| level_matches(o, &self.filter))
            .len()
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        active_level: Option<&str>,
        theme: &Theme,
    ) {
        let items = self.display_items(active_level);

        render_selectable_list_minimal(
            frame,
            area,
            "thinking",
            &items,
            self.list.selected,
            &self.filter,
            true,
            theme,
        );
    }
}

fn level_matches(option: &ThinkingOption, filter: &str) -> bool {
    filter.is_empty()
        || option.level.to_lowercase().contains(&filter.to_lowercase())
        || option
            .detail
            .to_lowercase()
            .contains(&filter.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_selects_active_level() {
        let mut picker = ThinkingSelector::new();
        picker.prepare(Some("high"));
        assert_eq!(picker.list.len(), THINKING_LEVELS.len());
        assert_eq!(picker.list.items[picker.list.selected].level, "high");
        assert_eq!(picker.confirm(), Some("high"));
    }

    #[test]
    fn confirm_filters_by_level() {
        let mut picker = ThinkingSelector::new();
        picker.prepare(None);
        picker.filter = "med".to_string();
        assert_eq!(picker.confirm(), Some("medium"));
    }
}
