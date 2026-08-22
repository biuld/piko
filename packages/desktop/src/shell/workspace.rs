//! Content-island header: agent TabGroup plus workspace toolbar (F-43).

use super::tabs::{
    agent_label, tab_items, tabs_disabled, truncate_chrome_label, view_target_requires_action,
};
use super::*;
use gpui::prelude::*;
use gpui::{AnyElement, App, ClickEvent, Window, div, px};
use island::components::chrome::{ChromeTextEmphasis, ChromeZones, GhostTextButton};
use island::components::panel::IslandHeader;
use island::components::tabs::TabGroup;

impl Shell {
    pub(super) fn timeline_header(&self, cx: &mut Context<Self>) -> Option<IslandHeader> {
        let items = tab_items(&self.state.core, self.pending_agent.as_deref());
        if items.is_empty() {
            return None;
        }
        let selected = self.view_key().map(str::to_string);
        let disabled = tabs_disabled(self.state.connection, self.layers.active().is_some());
        let entity = cx.entity().downgrade();
        let group = TabGroup::new("piko-agent-tabs", items)
            .selected(selected)
            .disabled(disabled)
            .focus_handle(self.agent_tabs_focus.clone())
            .material(self.material)
            .on_select(move |id: String, window: &mut Window, app: &mut App| {
                if let Some(shell) = entity.upgrade() {
                    shell.update(app, |shell, cx| {
                        shell.set_focus_owner(FocusOwner::AgentTabs, window, cx);
                        shell.select_agent(id, cx);
                    });
                }
            });
        Some(IslandHeader::chrome(
            ChromeZones::new(None, Some(self.workspace_toolbar(cx)), None)
                .principal(group)
                .principal_min_width(px(160.)),
        ))
    }

    fn workspace_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let model = self
            .state
            .core
            .model
            .model_id
            .clone()
            .unwrap_or_else(|| "Default model".to_string());
        let thinking = self
            .state
            .core
            .model
            .thinking_level
            .clone()
            .unwrap_or_else(|| "Default thinking".to_string());
        let initiating = self.focus_owner;
        let entity = cx.entity().downgrade();
        let model_click = move |_: &ClickEvent, window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| {
                    shell.open_layer(LayerKind::Model, initiating, cx);
                    let _ = window;
                });
            }
        };
        let entity = cx.entity().downgrade();
        let thinking_click = move |_: &ClickEvent, window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| {
                    shell.open_layer(LayerKind::Thinking, initiating, cx);
                    let _ = window;
                });
            }
        };
        let attention = view_target_requires_action(&self.state.core, self.view_key());
        let m = metrics();
        div()
            .flex()
            .items_center()
            .gap(m.space_xs)
            .child(
                GhostTextButton::new("piko-model", truncate_chrome_label(&model, 22))
                    .emphasis(ChromeTextEmphasis::Foreground)
                    .tooltip(format!("Session model (next turn) · {model}"))
                    .material(self.material)
                    .on_click(model_click),
            )
            .child(
                GhostTextButton::new("piko-thinking", truncate_chrome_label(&thinking, 16))
                    .emphasis(ChromeTextEmphasis::Foreground)
                    .tooltip("Session thinking level (next turn)")
                    .material(self.material)
                    .on_click(thinking_click),
            )
            .when(attention, |bar| {
                let entity = cx.entity().downgrade();
                bar.child(
                    GhostTextButton::new("piko-attention", "Needs attention")
                        .emphasis(ChromeTextEmphasis::Foreground)
                        .tooltip(
                            self.view_key()
                                .map(|id| {
                                    format!(
                                        "Needs attention · {}",
                                        agent_label(&self.state.core, id)
                                    )
                                })
                                .unwrap_or_else(|| "Needs attention".to_string()),
                        )
                        .material(self.material)
                        .on_click(move |_, _, app| {
                            if let Some(shell) = entity.upgrade() {
                                shell.update(app, |shell, cx| {
                                    shell.open_layer(LayerKind::Attention, initiating, cx);
                                });
                            }
                        }),
                )
            })
            .into_any_element()
    }
}
