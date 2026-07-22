//! Settings panel body — section header + scrollable form content.
//!
//! The form uses the **full width** of the right island (minus padding). Do not
//! pin a narrow reading column here — that leaves a large empty band on wide
//! windows and looks broken next to the nav.

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::theme::{label_text, metrics, tokens};

use super::SettingsSection;
use super::sections::render_section_panel;

/// Horizontal inset shared by header title and form (matches compact density).
fn content_pad_x() -> Pixels {
    metrics().space_lg // 16
}

/// Top padding under the section title; bottom uses a bit more for scroll end.
fn content_pad_top() -> Pixels {
    metrics().space_md // 12
}

pub fn render_panel(
    section: SettingsSection,
    app: &DesktopApp,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let pad_x = content_pad_x();

    div()
        .id("settings-panel-inner")
        .size_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        // ── Section title ────────────────────────────────────────────────
        .child(
            div()
                .id("settings-panel-header")
                .h(m.panel_header_height)
                .w_full()
                .px(pad_x)
                .flex()
                .items_center()
                .flex_shrink_0()
                .child(
                    label_text(true)
                        .text_color(t.fg_rgba())
                        .child(crate::t!(section.i18n_key())),
                ),
        )
        // ── Full-width scroll form ───────────────────────────────────────
        .child(
            div()
                .id("settings-panel")
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .overflow_y_scroll()
                .px(pad_x)
                .pt(content_pad_top())
                .pb(m.space_lg)
                // Stack content at the top; do not stretch groups to fill height.
                .flex()
                .flex_col()
                .justify_start()
                .child(
                    div()
                        .id("settings-panel-form")
                        .w_full()
                        .flex()
                        .flex_col()
                        .flex_shrink_0()
                        .gap(m.space_md)
                        .child(render_section_panel(section, app, entity)),
                ),
        )
}
