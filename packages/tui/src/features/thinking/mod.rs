//! Model configuration's second-stage thinking selector.
//!
//! Options come from the selected model's catalog capabilities. Non-reasoning
//! targets expose only `off`, so the workflow cannot create an unsupported
//! model/effort pair.

use ratatui::{Frame, layout::Rect};

use piko_protocol::model::ThinkingLevel;
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

#[derive(Clone, Debug)]
pub struct ThinkingOption {
    level: ThinkingLevel,
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
                    ColumnCell::primary(option.level.as_str()),
                    ColumnCell::secondary(option.detail),
                ])
                .active(active_level == Some(option.level.as_str()))
            })
            .collect()
    }
    pub fn new() -> Self {
        Self {
            list: SelectableList::new(Vec::new()),
            filter: String::new(),
        }
    }

    /// Rebuild the list from the exact target's advertised capabilities.
    pub fn prepare(&mut self, supported: &[ThinkingLevel], active: Option<&str>) {
        self.filter.clear();
        let levels = if supported.is_empty() {
            vec![ThinkingLevel::Off]
        } else {
            supported.to_vec()
        };
        self.list = SelectableList::new(
            levels
                .into_iter()
                .map(|level| ThinkingOption {
                    detail: thinking_level_detail(level.as_str()),
                    level,
                })
                .collect(),
        );
        if let Some(idx) = self
            .list
            .items
            .iter()
            .position(|option| active == Some(option.level.as_str()))
        {
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
    pub fn confirm(&self) -> Option<ThinkingLevel> {
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
            .map(|o| o.level.clone())
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
        || option
            .level
            .as_str()
            .to_lowercase()
            .contains(&filter.to_lowercase())
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
        let supported = vec![ThinkingLevel::Off, ThinkingLevel::High];
        picker.prepare(&supported, Some("high"));
        assert_eq!(picker.list.len(), 2);
        assert_eq!(
            picker.list.items[picker.list.selected].level,
            ThinkingLevel::High
        );
        assert_eq!(picker.confirm(), Some(ThinkingLevel::High));
    }

    #[test]
    fn confirm_filters_by_level() {
        let mut picker = ThinkingSelector::new();
        picker.prepare(&[ThinkingLevel::Low, ThinkingLevel::Medium], None);
        picker.filter = "med".to_string();
        assert_eq!(picker.confirm(), Some(ThinkingLevel::Medium));
    }

    #[test]
    fn empty_capabilities_offer_only_off() {
        let mut picker = ThinkingSelector::new();
        picker.prepare(&[], Some("low"));
        assert_eq!(picker.list.len(), 1);
        assert_eq!(picker.confirm(), Some(ThinkingLevel::Off));
    }
}
