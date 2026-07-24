//! Shared overlay surface: dimmed backdrop + centered panel variants.
//!
//! Geometry is resolved by [`super::envelope::overlay_envelope`] so panels fit
//! narrow/short windows. Body scrolls inside the max-height box.

use gpui::prelude::FluentBuilder;
use gpui::*;

use super::envelope::overlay_envelope;
use crate::theme::{TextRole, metrics, text, tokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPanelStyle {
    /// HostPrompt / LocalConfirm: padded elevated dialog.
    Dialog,
    /// Command Palette: compact island-like panel; search is the primary header.
    Palette,
}

pub struct OverlayPanelSpec {
    pub title: SharedString,
    /// Preferred panel width; chrome clamps to the viewport.
    pub width: Pixels,
    /// Window content size when known (from `window.viewport_size()`).
    pub viewport: Option<Size<Pixels>>,
    /// When true, backdrop click dismisses via `on_backdrop`.
    pub backdrop_dismiss: bool,
    pub style: OverlayPanelStyle,
}

/// Full-window absolute overlay shell. `body` is the panel content below the
/// title (Dialog) or the full compact content (Palette).
pub fn render_overlay_layer(
    panel: OverlayPanelSpec,
    body: impl IntoElement,
    on_backdrop: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let t = tokens();
    let m = metrics();
    let title = panel.title;
    let env = overlay_envelope(panel.width, panel.style, panel.viewport);
    let width = env.width;
    let max_h = env.max_height;
    let top_pad = env.top_pad;
    let backdrop_dismiss = panel.backdrop_dismiss;
    let style = panel.style;
    let show_crumb = style == OverlayPanelStyle::Palette && !title.is_empty();
    let dim = match style {
        OverlayPanelStyle::Dialog => hsla(0.0, 0.0, 0.0, 0.45),
        OverlayPanelStyle::Palette => hsla(0.0, 0.0, 0.0, 0.5),
    };

    let body = div()
        .id("chrome-overlay-body")
        .w_full()
        .flex_1()
        .min_h(px(0.))
        .overflow_y_scroll()
        .child(body);

    let panel_el = match style {
        OverlayPanelStyle::Dialog => div()
            .id("chrome-overlay-panel")
            .occlude()
            .w(width)
            .max_w(width)
            .max_h(max_h)
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_3()
            .p_6()
            .rounded_lg()
            .border_1()
            .border_color(t.border_rgba())
            .bg(t.elevated_rgba())
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(
                text(TextRole::PlaceholderTitle)
                    .text_color(t.fg_rgba())
                    .flex_shrink_0()
                    .child(title),
            )
            .child(body)
            .into_any_element(),
        OverlayPanelStyle::Palette => div()
            .id("chrome-overlay-palette-panel")
            .occlude()
            .w(width)
            .max_w(width)
            .max_h(max_h)
            .overflow_hidden()
            .flex()
            .flex_col()
            .rounded(m.island_radius)
            .border_1()
            .border_color(t.border_rgba())
            .bg(t.surface_rgba())
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .when(show_crumb, |d| {
                d.child(
                    div()
                        .id("chrome-overlay-palette-crumb")
                        .h(px(28.))
                        .px(m.tool_row_inset)
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .border_b_1()
                        .border_color(t.border_rgba())
                        .child(
                            text(TextRole::Meta)
                                .text_color(t.muted_fg_rgba())
                                .child(title),
                        ),
                )
            })
            .child(body)
            .into_any_element(),
    };

    div()
        .id("chrome-overlay-layer")
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_start()
        .justify_center()
        .pt(top_pad)
        .pb(env.bottom_pad)
        .px(env.h_margin)
        .bg(dim)
        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
            cx.stop_propagation();
            if backdrop_dismiss {
                on_backdrop(ev, window, cx);
            }
        })
        .child(panel_el)
        .into_any_element()
}
