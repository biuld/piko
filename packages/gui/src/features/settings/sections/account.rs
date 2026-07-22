//! Account & Providers — auth status from the model catalog.

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::theme::{TextRole, metrics, text, tokens};

use super::super::widgets::{section_lede, setting_group, status_badge, text_button};

pub fn render_account(app: &DesktopApp, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    let t = tokens();
    let providers = &app.bridge_state().model.providers;

    div()
        .w_full()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(m.space_md)
        .child(section_lede(crate::t!("settings.account.lede")))
        .child(setting_group(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(m.space_md)
                .when(providers.is_empty(), |panel| {
                    panel.child(section_lede(crate::t!("settings.account.providers.empty")))
                })
                .children(providers.iter().enumerate().map(|(ix, provider)| {
                    let status = if provider.has_auth {
                        crate::t!("settings.account.providers.authenticated")
                    } else {
                        crate::t!("settings.account.providers.not_authenticated")
                    };
                    div()
                        .id(SharedString::from(format!("settings-provider-{ix}")))
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(m.space_lg)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    crate::theme::label_text(false)
                                        .text_color(t.fg_rgba())
                                        .child(provider.provider.clone()),
                                )
                                .child(text(TextRole::Meta).text_color(t.muted_fg_rgba()).child(
                                    format!(
                                        "{} {}",
                                        provider.models.len(),
                                        crate::t!("settings.account.providers.models")
                                    ),
                                )),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .child(status_badge(status, provider.has_auth)),
                        )
                }))
                .child(section_lede(crate::t!("settings.account.auth_hint")))
                .child({
                    let entity = entity.clone();
                    text_button(
                        "settings-account-refresh-providers",
                        crate::t!("settings.account.refresh"),
                        move |_, _, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.settings_refresh_models(cx);
                                });
                            }
                        },
                    )
                }),
        ))
}
