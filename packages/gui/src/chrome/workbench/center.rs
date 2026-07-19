//! Center column: Timeline + Composer island Entities.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::chrome::island::{IslandPanel, IslandPlaceholder};
use crate::projections::{SessionPhaseView, derive_phase_view};
use crate::theme::{PikoIcon, metrics};

pub(crate) fn render_center(
    app: &DesktopApp,
    _window: &mut Window,
    _cx: &mut Context<DesktopApp>,
) -> AnyElement {
    let phase = derive_phase_view(app.bridge_state());
    let m = metrics();

    let timeline_slot = match phase {
        SessionPhaseView::Live => app.timeline.clone().into_any_element(),
        SessionPhaseView::IdleNoSession => IslandPanel::empty(
            "pre-live-island",
            IslandPlaceholder::new(crate::t!("center.no_session.title"))
                .piko_icon(PikoIcon::Circle)
                .subtitle(crate::t!("center.no_session.subtitle")),
        )
        .scroll(false)
        .into_any_element(),
        SessionPhaseView::Opening { .. } => IslandPanel::loading(
            "pre-live-island",
            IslandPlaceholder::new(crate::t!("center.opening")).piko_icon(PikoIcon::CircleDashed),
        )
        .into_any_element(),
        SessionPhaseView::Hydrating { .. } => IslandPanel::loading(
            "pre-live-island",
            IslandPlaceholder::new(crate::t!("center.loading")).piko_icon(PikoIcon::CircleDashed),
        )
        .into_any_element(),
        SessionPhaseView::Error { message } => IslandPanel::empty(
            "error-island",
            IslandPlaceholder::new(crate::t!("center.error.title"))
                .piko_icon(PikoIcon::TriangleAlert)
                .subtitle(message),
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
                .child(div().flex_1().min_h(px(0.)).child(timeline_slot)),
        )
        .child(app.composer.clone())
        .into_any_element()
}
