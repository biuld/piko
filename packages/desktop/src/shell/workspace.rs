//! Window-toolbar TabGroup and model/thinking tools (F-43).

use super::pickers::{PickerEntry, PickerPayload, model_entries, thinking_entries};
use super::tabs::{
    agent_label, model_chrome_label, tab_items, tabs_disabled, thinking_chrome_label,
    view_target_requires_action,
};
use super::*;
use gpui::prelude::*;
use gpui::{AnyElement, App, WeakEntity, Window};
use island::components::chrome::{ChromeMenuButton, ChromeTextEmphasis, GhostTextButton};
use island::components::menu::{ContextMenuItem, ContextMenuSpec};
use island::components::tabs::TabGroup;
use island::theme::IslandIcon;

impl Shell {
    pub(super) fn agent_tab_group(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let items = tab_items(&self.state.core, self.pending_agent.as_deref());
        if items.is_empty() {
            return None;
        }
        let selected = self.view_key().map(str::to_string);
        let disabled = tabs_disabled(self.state.connection, self.layers.active().is_some());
        let entity = cx.entity().downgrade();
        Some(
            TabGroup::new("piko-agent-tabs", items)
                .selected(selected)
                .disabled(disabled)
                .clustered(true)
                .focus_handle(self.agent_tabs_focus.clone())
                .material(self.material)
                .on_select(move |id: String, window: &mut Window, app: &mut App| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| {
                            shell.set_focus_owner(FocusOwner::AgentTabs, window, cx);
                            shell.select_agent(id, cx);
                        });
                    }
                })
                .into_any_element(),
        )
    }

    pub(super) fn model_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let model = self
            .state
            .core
            .model
            .model_id
            .clone()
            .unwrap_or_else(|| "Model".to_string());
        let entity = cx.entity().downgrade();
        ChromeMenuButton::new("piko-model", model_chrome_label(&model))
            .emphasis(ChromeTextEmphasis::Foreground)
            .leading_icon(IslandIcon::Cpu)
            .trailing_icon(IslandIcon::ChevronDown)
            .capsule(true)
            .tooltip(format!("Session model (next turn) · {model}"))
            .material(self.material)
            .menu(move |_, app| {
                let Some(shell) = entity.upgrade() else {
                    return ContextMenuSpec::new([]);
                };
                let entries = model_entries(&shell.read(app).state.core.model);
                let material = shell.read(app).material;
                ContextMenuSpec::new(map_picker_entries(entries, entity.clone())).material(material)
            })
            .into_any_element()
    }

    pub(super) fn thinking_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let thinking = self
            .state
            .core
            .model
            .thinking_level
            .clone()
            .unwrap_or_else(|| "off".to_string());
        let entity = cx.entity().downgrade();
        ChromeMenuButton::new("piko-thinking", thinking_chrome_label(&thinking))
            .emphasis(ChromeTextEmphasis::Foreground)
            .leading_icon(IslandIcon::Brain)
            .trailing_icon(IslandIcon::ChevronDown)
            .capsule(true)
            .tooltip(format!("Session thinking level (next turn) · {thinking}"))
            .material(self.material)
            .menu(move |_, app| {
                let Some(shell) = entity.upgrade() else {
                    return ContextMenuSpec::new([]);
                };
                let entries =
                    thinking_entries(shell.read(app).state.core.model.thinking_level.as_deref());
                let material = shell.read(app).material;
                ContextMenuSpec::new(map_picker_entries(entries, entity.clone())).material(material)
            })
            .into_any_element()
    }

    pub(super) fn attention_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !view_target_requires_action(&self.state.core, self.view_key()) {
            return None;
        }
        let initiating = self.focus_owner;
        let entity = cx.entity().downgrade();
        let tooltip = self
            .view_key()
            .map(|id| format!("Needs attention · {}", agent_label(&self.state.core, id)))
            .unwrap_or_else(|| "Needs attention".to_string());
        Some(
            GhostTextButton::new("piko-attention", "Needs attention")
                .emphasis(ChromeTextEmphasis::Foreground)
                .leading_icon(IslandIcon::TriangleAlert)
                .capsule(true)
                .tooltip(tooltip)
                .material(self.material)
                .on_click(move |_, _, app| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| {
                            shell.open_layer(LayerKind::Attention, initiating, cx);
                        });
                    }
                })
                .into_any_element(),
        )
    }
}

fn map_picker_entries(
    entries: Vec<PickerEntry>,
    entity: WeakEntity<Shell>,
) -> Vec<ContextMenuItem> {
    entries
        .into_iter()
        .map(|entry| map_picker_entry(entry, entity.clone()))
        .collect()
}

fn map_picker_entry(entry: PickerEntry, entity: WeakEntity<Shell>) -> ContextMenuItem {
    match entry {
        PickerEntry::Header { label } => ContextMenuItem::action(label, |_, _| {}).enabled(false),
        PickerEntry::Separator => ContextMenuItem::separator(),
        PickerEntry::Action {
            label,
            selected,
            enabled,
            payload,
        } => ContextMenuItem::action(label, move |window, app| {
            dispatch_picker_payload(&entity, payload.clone(), window, app);
        })
        .selected(selected)
        .enabled(enabled),
    }
}

fn dispatch_picker_payload(
    entity: &WeakEntity<Shell>,
    payload: PickerPayload,
    _window: &mut Window,
    app: &mut App,
) {
    let Some(shell) = entity.upgrade() else {
        return;
    };
    let intent = match payload {
        PickerPayload::SetModel { provider, model_id } => {
            ClientIntent::SetModel { provider, model_id }
        }
        PickerPayload::SetThinking(level) => ClientIntent::SetThinkingLevel { level },
        PickerPayload::None => return,
    };
    shell.update(app, |shell, cx| {
        shell.dispatch_intents(cx, vec![intent]);
    });
}
