//! Compact notification history presentation.
//!
//! Applications own records, unread policy, localization, and toast lifecycle.
//! Chrome owns responsive surface geometry, anchoring, and row presentation.

mod api;

pub use api::{
    NotificationBellSpec, NotificationToastSpec, clear_notification_toasts,
    push_notification_toast, render_notification_bell, render_notification_center_layer,
    render_notification_toasts,
};

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Sizable;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::scroll::ScrollableElement;

use crate::theme::{ChromeTokens, TextRole, label_text, metrics, text, tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationSeverity {
    fn color(self, tokens: ChromeTokens) -> Rgba {
        match self {
            Self::Info => ChromeTokens::rgba(tokens.info),
            Self::Success => ChromeTokens::rgba(tokens.success),
            Self::Warning => ChromeTokens::rgba(tokens.warning),
            Self::Error => ChromeTokens::rgba(tokens.danger),
        }
    }
}

pub struct NotificationPanelSpec {
    pub title: SharedString,
    pub clear_label: SharedString,
    pub empty_title: SharedString,
    pub viewport: Size<Pixels>,
}

pub struct NotificationRowSpec {
    pub id: ElementId,
    pub remove_id: ElementId,
    pub severity: NotificationSeverity,
    pub title: SharedString,
    pub message: SharedString,
    pub time: SharedString,
    pub remove_label: SharedString,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotificationSurfaceLayout {
    pub panel_width: Pixels,
    pub panel_max_height: Pixels,
    pub top: Pixels,
    pub right: Pixels,
}

const PANEL_PREFERRED_WIDTH: f32 = 360.;
const PANEL_MAX_HEIGHT: f32 = 520.;
const PANEL_MIN_WIDTH: f32 = 280.;
const PANEL_MIN_HEIGHT: f32 = 220.;

/// Resolve notification surfaces from chrome-owned TitleBar and gutter
/// metrics. Product state never changes the toast stack position.
pub fn notification_surface_layout(viewport: Size<Pixels>) -> NotificationSurfaceLayout {
    let m = metrics();
    let viewport_width = f32::from(viewport.width);
    let viewport_height = f32::from(viewport.height);
    let gutter = f32::from(m.island_gutter);
    let title_height = f32::from(m.title_bar_height);
    let available_width = (viewport_width - 2. * gutter).max(0.);
    let panel_width = PANEL_PREFERRED_WIDTH
        .min(available_width)
        .max(PANEL_MIN_WIDTH.min(available_width));
    let available_height = (viewport_height - title_height - 2. * gutter).max(0.);
    let panel_max_height = PANEL_MAX_HEIGHT
        .min(available_height)
        .max(PANEL_MIN_HEIGHT.min(available_height));

    NotificationSurfaceLayout {
        panel_width: px(panel_width),
        panel_max_height: px(panel_max_height),
        top: m.title_bar_height + m.island_gutter,
        right: m.island_gutter,
    }
}

/// Anchor a notification center panel below the trailing TitleBar actions.
pub fn render_notification_panel_anchor(panel: impl IntoElement) -> AnyElement {
    let m = metrics();
    div()
        .absolute()
        .top(m.title_bar_height + m.island_gutter)
        .right(m.island_gutter)
        .child(panel)
        .into_any_element()
}

/// Anchor an application's existing toast stack at the stable chrome position.
pub fn render_notification_toast_layer(toasts: impl IntoElement) -> AnyElement {
    let m = metrics();
    div()
        .absolute()
        .top(m.title_bar_height + m.island_gutter)
        .right(m.island_gutter)
        .child(toasts)
        .into_any_element()
}

/// Elevated notification history frame. Use
/// [`render_notification_panel_anchor`] for standard placement; click-away
/// handling intentionally remains application policy.
pub fn render_notification_panel(
    spec: NotificationPanelSpec,
    rows: Vec<AnyElement>,
    on_clear: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let t = tokens();
    let m = metrics();
    let is_empty = rows.is_empty();
    let layout = notification_surface_layout(spec.viewport);

    div()
        .id("chrome-notification-panel")
        .occlude()
        .w(layout.panel_width)
        .max_h(layout.panel_max_height)
        .overflow_hidden()
        .flex()
        .flex_col()
        .rounded(m.island_radius)
        .border_1()
        .border_color(t.border_rgba())
        .bg(t.elevated_rgba())
        .shadow_lg()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .h(m.panel_header_height)
                .flex_shrink_0()
                .px(m.space_md)
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(t.border_rgba())
                .child(label_text(true).text_color(t.fg_rgba()).child(spec.title))
                .when(!is_empty, |header| {
                    header.child(
                        Button::new("notification-clear-all")
                            .label(spec.clear_label)
                            .ghost()
                            .xsmall()
                            .on_click(on_clear),
                    )
                }),
        )
        .child(if is_empty {
            div()
                .h(px(160.))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    text(TextRole::PlaceholderSubtitle)
                        .text_color(t.muted_fg_rgba())
                        .child(spec.empty_title),
                )
                .into_any_element()
        } else {
            div()
                .min_h(px(0.))
                .overflow_y_scrollbar()
                .children(rows)
                .into_any_element()
        })
        .into_any_element()
}

/// One compact history row with a semantic status mark and optional removal.
pub fn render_notification_row(
    spec: NotificationRowSpec,
    on_remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let t = tokens();
    let m = metrics();
    let accent = spec.severity.color(t);

    div()
        .id(spec.id)
        .group("notification-row")
        .relative()
        .w_full()
        .px(m.space_md)
        .py(m.space_sm)
        .border_b_1()
        .border_color(t.border_rgba())
        .flex()
        .items_start()
        .gap(m.space_sm)
        .child(
            div()
                .mt(px(5.))
                .size(px(7.))
                .flex_shrink_0()
                .rounded_full()
                .bg(accent),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(m.space_xs)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(m.space_sm)
                        .child(
                            label_text(true)
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_color(t.fg_rgba())
                                .child(spec.title),
                        )
                        .child(
                            text(TextRole::Meta)
                                .flex_shrink_0()
                                .text_color(t.muted_fg_rgba())
                                .child(spec.time),
                        ),
                )
                .child(
                    text(TextRole::Meta)
                        .text_color(t.muted_fg_rgba())
                        .child(spec.message),
                ),
        )
        .child(
            Button::new(spec.remove_id)
                .label(spec.remove_label)
                .ghost()
                .xsmall()
                .flex_shrink_0()
                .on_click(on_remove),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{NotificationSeverity, notification_surface_layout};
    use crate::theme::ChromeTokens;
    use gpui::{px, size};

    #[test]
    fn severity_uses_semantic_tokens() {
        let t = ChromeTokens::dark();
        assert_eq!(
            NotificationSeverity::Info.color(t),
            ChromeTokens::rgba(t.info)
        );
        assert_eq!(
            NotificationSeverity::Success.color(t),
            ChromeTokens::rgba(t.success)
        );
        assert_eq!(
            NotificationSeverity::Warning.color(t),
            ChromeTokens::rgba(t.warning)
        );
        assert_eq!(
            NotificationSeverity::Error.color(t),
            ChromeTokens::rgba(t.danger)
        );
    }

    #[test]
    fn surfaces_follow_title_bar_and_gutter_metrics() {
        let layout = notification_surface_layout(size(px(1200.), px(800.)));
        assert_eq!(layout.panel_width, px(360.));
        assert_eq!(layout.panel_max_height, px(520.));
        assert_eq!(layout.top, px(42.));
        assert_eq!(layout.right, px(8.));
    }

    #[test]
    fn panel_width_clamps_to_narrow_viewport() {
        let layout = notification_surface_layout(size(px(300.), px(600.)));
        assert_eq!(layout.panel_width, px(284.));
        assert!(layout.panel_max_height <= px(520.));
    }
}
