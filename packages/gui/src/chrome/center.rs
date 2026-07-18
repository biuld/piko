//! Center column assembly: Timeline + Composer island Entities.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_actions::render_pane_toggles;
use crate::chrome::{IslandPanel, IslandPlaceholder};
use crate::projections::{SessionPhaseView, derive_phase_view};
use crate::theme::metrics;

pub(crate) fn render_center(
    app: &DesktopApp,
    _window: &mut Window,
    cx: &mut Context<DesktopApp>,
) -> AnyElement {
    let phase = derive_phase_view(app.bridge_state());
    let entity = cx.entity().downgrade();
    let m = metrics();

    let timeline_slot = match phase {
        SessionPhaseView::Live => app.timeline.clone().into_any_element(),
        SessionPhaseView::IdleNoSession => IslandPanel::empty(
            "pre-live-island",
            IslandPlaceholder::new("No session")
                .icon("○")
                .subtitle("Select a session or type to start one."),
        )
        .scroll(false)
        .into_any_element(),
        SessionPhaseView::Opening { .. } => IslandPanel::loading(
            "pre-live-island",
            IslandPlaceholder::new("Opening session…").icon("◌"),
        )
        .into_any_element(),
        SessionPhaseView::Hydrating { .. } => IslandPanel::loading(
            "pre-live-island",
            IslandPlaceholder::new("Loading session…").icon("◌"),
        )
        .into_any_element(),
        SessionPhaseView::Error { message } => IslandPanel::empty(
            "error-island",
            IslandPlaceholder::new("Error").icon("!").subtitle(message),
        )
        .scroll(false)
        .into_any_element(),
    };

    div()
        .size_full()
        .h_full()
        .flex()
        .flex_col()
        .gap(m.island_gutter)
        .overflow_hidden()
        .child(
            div()
                .flex_1()
                .min_h(px(0.))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(render_pane_toggles(app, entity))
                .child(div().flex_1().min_h(px(0.)).child(timeline_slot)),
        )
        .child(app.composer.clone())
        .into_any_element()
}
