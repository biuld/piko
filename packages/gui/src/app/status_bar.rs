//! StatusBar rendering — always visible, single-line, read-only.
//!
//! Stable order: connection, optional cwd, context/cost.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::shell::{ConnectionStatus, StatusBarViewModel};
use crate::theme::{RoleAccent, metrics, tokens};

pub fn render_status_bar(vm: &StatusBarViewModel, allow_motion: bool) -> Div {
    let t = tokens();
    let m = metrics();
    let content = div()
        .relative()
        .top(m.status_content_offset_y)
        .h(m.meta_line_height)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .gap(m.space_sm)
        .text_size(m.meta_size)
        .line_height(m.meta_line_height)
        .child(render_connection(&vm.connection, allow_motion))
        .when_some(vm.cwd.clone(), |d, cwd| {
            d.child(div().text_color(t.muted_fg_rgba()).child(cwd))
        })
        .when_some(vm.usage.clone(), |d, usage| {
            d.child(
                div()
                    .ml_auto()
                    .text_color(t.role_accent(RoleAccent::Info))
                    .child(usage),
            )
        });

    div()
        .h(m.status_bar_height)
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .px(m.chrome_horizontal_padding)
        .bg(t.chrome_rgba())
        .child(content)
}

fn render_connection(status: &ConnectionStatus, allow_motion: bool) -> Div {
    let t = tokens();
    let (color, label) = match status {
        ConnectionStatus::Connected => (t.role_accent(RoleAccent::Success), "hostd connected"),
        ConnectionStatus::Disconnected => (t.role_accent(RoleAccent::Danger), "hostd disconnected"),
    };

    // Reduced motion: square indicator instead of a soft pill.
    let dot = div().w(px(6.0)).h(px(6.0)).bg(color);
    let dot = if allow_motion {
        dot.rounded_full()
    } else {
        dot.rounded(px(1.))
    };

    div()
        .h(metrics().meta_line_height)
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(dot)
        .child(div().text_color(color).child(label))
}
