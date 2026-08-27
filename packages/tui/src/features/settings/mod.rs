//! Settings panel — surface composition over the shared MenuStack + Pane.

mod catalog;
mod mirror;

pub use catalog::{SettingsAction, action_requires_hostd_restart, thinking_level_detail};
pub use mirror::{HostRuntimeSettings, SettingsSnapshot};

use ratatui::{Frame, layout::Rect};

use piko_tui_layout::{Component, InteractionState, SurfacePanel};

use crate::{
    app::{HitId, command::SurfaceAction},
    navigation::SurfaceId,
    theme::Theme,
    ui::components::{
        menu::{MenuConfirmResult, MenuRowLayout, MenuStack},
        pane::{PaneAffixHit, PaneFooter, PaneMode, PaneSpec, PaneTitleAffix},
        selectable_list::{
            SelectableItem, paint_row_hover, render_selectable_list_with_pane,
            selectable_row_regions,
        },
    },
    ui::interaction::{ComponentHit, PointerComponent, PointerGesture, paint_element_hover},
};

pub struct SettingsCtx<'a> {
    pub theme: &'a Theme,
    pub hints: Option<&'a str>,
}

impl Component<HitId, SettingsCtx<'_>> for SettingsPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &SettingsCtx<'_>) {
        self.render(frame, area, ctx.theme, ctx.hints);
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &SettingsCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx.theme, ctx.hints);
        let regions = self.row_regions(area);
        paint_row_hover(
            frame,
            &regions,
            interaction,
            self.stack.selected_index(),
            ctx.theme,
        );
        paint_element_hover(
            frame,
            &self.close_region(area),
            interaction,
            None,
            ctx.theme,
        );
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let mut regions: Vec<_> = self
            .row_regions(area)
            .into_iter()
            .map(|(rect, i)| (rect, HitId::Row(i)))
            .collect();
        regions.extend(self.close_region(area));
        regions
    }
}

impl PointerComponent<HitId> for SettingsPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        match (gesture, hit.element) {
            (PointerGesture::Activate, Some(HitId::Row(i))) => {
                self.stack.select_index(i);
                vec![SurfaceAction::Confirm.into()]
            }
            (PointerGesture::Activate, Some(HitId::Close)) => vec![SurfaceAction::Close.into()],
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

impl SurfacePanel<SurfaceId, HitId, SettingsCtx<'_>> for SettingsPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Settings
    }
}

use catalog::build_catalog;

/// Result of confirming a settings row (compatible name for session_ops).
pub type SettingsConfirmResult = MenuConfirmResult<SettingsAction>;

/// Settings panel: MenuStack + filter + catalog built from snapshot.
pub struct SettingsPanel {
    pub stack: MenuStack<SettingsAction>,
    pub filter: String,
}

impl SettingsPanel {
    fn close_region(&self, area: Rect) -> Vec<(Rect, HitId)> {
        if !self.stack.at_root() {
            return Vec::new();
        }
        PaneSpec::new("Settings")
            .affix(PaneTitleAffix::Close)
            .title_affix_regions(area)
            .into_iter()
            .filter_map(|(rect, hit)| (hit == PaneAffixHit::Close).then_some((rect, HitId::Close)))
            .collect()
    }

    fn row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        let Some(current) = self.stack.current() else {
            return Vec::new();
        };
        let at_root = self.stack.at_root();
        let title = if at_root { "Settings" } else { &current.title };
        let mut spec = PaneSpec::new(title)
            .mode(PaneMode::Standard)
            .search_filter(&self.filter)
            .search_rule(true)
            // Hit testing only needs the one-row footer budget. Render text
            // is supplied by the binding-derived context.
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true);
        if at_root {
            spec = spec.affix(PaneTitleAffix::Close);
        }
        selectable_row_regions(
            area,
            &spec,
            &self.stack.display_items(),
            self.stack.selected_index(),
            &self.filter,
        )
    }
    pub fn new() -> Self {
        Self {
            stack: MenuStack::new(),
            filter: String::new(),
        }
    }

    pub fn open_root(&mut self, snap: &SettingsSnapshot) {
        self.filter.clear();
        self.stack
            .open("settings", MenuRowLayout::SettingsRow, build_catalog(snap));
    }

    pub fn pop(&mut self) -> bool {
        self.filter.clear();
        self.stack.pop()
    }

    pub fn select_next(&mut self) {
        self.stack.select_next(&self.filter);
    }

    pub fn select_prev(&mut self) {
        self.stack.select_prev(&self.filter);
    }

    pub fn reset_selection(&mut self) {
        self.stack.reset_selection();
    }

    pub fn confirm(&mut self) -> SettingsConfirmResult {
        self.stack.confirm(&mut self.filter)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme, hints: Option<&str>) {
        let Some(current) = self.stack.current() else {
            return;
        };
        let at_root = self.stack.at_root();
        let title = if at_root { "Settings" } else { &current.title };
        let items: Vec<SelectableItem> = current
            .items
            .items
            .iter()
            .map(|row| row.to_item(current.layout))
            .collect();
        let mut spec = PaneSpec::new(title)
            .mode(PaneMode::Standard)
            .search_filter(&self.filter)
            .search_rule(true)
            .focused(true);
        if let Some(hints) = hints.filter(|value| !value.is_empty()) {
            spec = spec.hints(hints);
        } else {
            spec = spec.footer(PaneFooter::Reserved { height: 1 });
        }
        if at_root {
            spec = spec.affix(PaneTitleAffix::Close);
        }
        render_selectable_list_with_pane(
            frame,
            area,
            spec,
            &items,
            current.items.selected,
            &self.filter,
            theme,
        );
    }
}

impl Default for SettingsPanel {
    fn default() -> Self {
        Self::new()
    }
}
