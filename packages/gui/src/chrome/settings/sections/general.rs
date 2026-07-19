//! General settings — host defaults and GUI thinking visibility.

use gpui::prelude::FluentBuilder;
use gpui::*;
use piko_protocol::ThinkingLevel;

use crate::app::desktop_app::DesktopApp;
use crate::app::model_cycle::{THINKING_LEVELS, catalog_models};
use crate::chrome::settings::widgets::{
    bool_switch, model_row, section_lede, selectable_chip, setting_group, setting_row, text_button,
    value_chip,
};
use crate::theme::metrics;

pub fn render_general(app: &DesktopApp, entity: WeakEntity<DesktopApp>) -> impl IntoElement {
    let m = metrics();
    let model = &app.bridge_state().model;
    let current_provider = model.provider.clone().unwrap_or_default();
    let current_model = model.model_id.clone().unwrap_or_default();
    let current_thinking = model
        .thinking_level
        .clone()
        .unwrap_or_else(|| ThinkingLevel::Off.as_str().to_string());

    let model_display = match (model.provider.as_deref(), model.model_id.as_deref()) {
        (Some(p), Some(m)) => format!("{p}/{m}"),
        (None, Some(m)) => m.to_string(),
        _ => crate::t!("settings.general.model.unset"),
    };

    let models = catalog_models(&model.providers);
    let models_empty = models.is_empty();

    div()
        .flex()
        .flex_col()
        .gap(m.space_lg)
        .child(section_lede(crate::t!("settings.general.lede")))
        .child(setting_group(
            div()
                .flex()
                .flex_col()
                .gap(m.space_md)
                .child(setting_row(
                    "settings-general-model-current",
                    crate::t!("settings.general.model.label"),
                    Some(crate::t!("settings.general.model.detail").into()),
                    value_chip(model_display),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            crate::theme::label_text(false)
                                .child(crate::t!("settings.general.model.catalog")),
                        )
                        .child({
                            let entity = entity.clone();
                            text_button(
                                "settings-general-refresh-models",
                                crate::t!("settings.general.model.refresh"),
                                move |_, _, cx| {
                                    if let Some(view) = entity.upgrade() {
                                        view.update(cx, |this, cx| {
                                            this.settings_refresh_models(cx);
                                        });
                                    }
                                },
                            )
                        }),
                )
                .when(models_empty, |panel| {
                    panel.child(section_lede(crate::t!("settings.general.model.empty")))
                })
                .children(models.into_iter().enumerate().map(
                    |(ix, (provider, model_id, name))| {
                        let selected = provider == current_provider && model_id == current_model;
                        let entity = entity.clone();
                        let provider_for_click = provider.clone();
                        let model_id_for_click = model_id.clone();
                        model_row(
                            SharedString::from(format!("settings-model-{ix}")),
                            name,
                            format!("{provider}/{model_id}"),
                            selected,
                            move |_, _, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.settings_set_model(
                                            provider_for_click.clone(),
                                            model_id_for_click.clone(),
                                            cx,
                                        );
                                    });
                                }
                            },
                        )
                    },
                )),
        ))
        .child(setting_group(
            div()
                .flex()
                .flex_col()
                .gap(m.space_sm)
                .child(setting_row(
                    "settings-general-thinking-current",
                    crate::t!("settings.general.thinking.label"),
                    Some(crate::t!("settings.general.thinking.detail").into()),
                    value_chip(current_thinking.clone()),
                ))
                .child(div().flex().flex_wrap().gap(m.space_xs).children(
                    THINKING_LEVELS.iter().enumerate().map(|(ix, level)| {
                        let selected = level.as_str() == current_thinking;
                        let entity = entity.clone();
                        let level_for_click = level.clone();
                        let label = level.as_str().to_string();
                        selectable_chip(
                            SharedString::from(format!("settings-thinking-{ix}")),
                            label,
                            selected,
                            move |_, _, cx| {
                                if let Some(view) = entity.upgrade() {
                                    view.update(cx, |this, cx| {
                                        this.settings_set_thinking_level(
                                            level_for_click.clone(),
                                            cx,
                                        );
                                    });
                                }
                            },
                        )
                    }),
                )),
        ))
        .child(setting_row(
            "settings-general-hide-thinking",
            crate::t!("settings.general.hide_thinking.label"),
            Some(crate::t!("settings.general.hide_thinking.detail").into()),
            {
                let checked = app.ux_prefs.hide_thinking_block;
                let entity = entity.clone();
                bool_switch(
                    "settings-general-hide-thinking-switch",
                    checked,
                    move |checked, _, cx| {
                        if let Some(view) = entity.upgrade() {
                            view.update(cx, |this, cx| {
                                this.settings_set_hide_thinking_block(checked, cx);
                            });
                        }
                    },
                )
            },
        ))
}
