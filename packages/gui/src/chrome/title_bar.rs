//! Native-integrated custom title bar for the desktop window chrome.

use gpui::*;
use gpui_component::Sizable;
use gpui_component::TitleBar;
use gpui_component::button::{Button, ButtonVariants};

use crate::app::desktop_app::{DesktopApp, ToggleRightColumn, ToggleSessions};
use crate::theme::{PanelSide, PikoTokens, label_text, metrics, panel_toggle_icon, tokens};
use piko_client_core::ClientState;

pub fn render_title_bar(
    state: &ClientState,
    project_name: &str,
    sessions_docked: bool,
    right_docked: bool,
    entity: WeakEntity<DesktopApp>,
) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let context = state
        .live_session
        .as_ref()
        .and_then(|session| session.name.clone())
        .unwrap_or_else(|| project_name.to_string());

    let sessions_toggle = {
        let entity = entity.clone();
        panel_toggle(
            "title-toggle-sessions",
            PanelSide::Left,
            sessions_docked,
            crate::t!("chrome.toggle.sessions"),
            move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.action_toggle_sessions(&ToggleSessions, window, cx);
                    });
                }
            },
        )
    };

    let right_toggle = {
        let entity = entity.clone();
        panel_toggle(
            "title-toggle-right-column",
            PanelSide::Right,
            right_docked,
            crate::t!("chrome.toggle.right_column"),
            move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.action_toggle_right_column(&ToggleRightColumn, window, cx);
                    });
                }
            },
        )
    };

    TitleBar::new().h(m.title_bar_height).child(
        div()
            .relative()
            .size_full()
            .pr(m.title_bar_safe_inset)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .gap(m.space_xs)
                    .child(sessions_toggle)
                    .child(right_toggle),
            )
            .child(
                label_text(false)
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(m.space_sm)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.fg_rgba())
                            .child("piko"),
                    )
                    .child(div().text_color(t.muted_fg_rgba()).child("/"))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(t.muted_fg_rgba())
                            .child(context),
                    ),
            ),
    )
}

/// Ghost icon button; open/closed is carried only by hollow vs hatched SVG.
fn panel_toggle(
    id: impl Into<ElementId>,
    side: PanelSide,
    docked: bool,
    tooltip: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Button {
    let color = PikoTokens::hsla(tokens().muted_fg);
    Button::new(id)
        .icon(panel_toggle_icon(side, docked, color))
        .tooltip(tooltip)
        .ghost()
        .small()
        .compact()
        .on_click(on_click)
}
