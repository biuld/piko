//! Content-island header: agent TabGroup on IslandPanel chrome (F-43).

use super::tabs::{tab_items, tabs_disabled};
use super::*;
use gpui::prelude::*;
use gpui::{App, Window};
use island::components::chrome::ChromeZones;
use island::components::panel::IslandHeader;
use island::components::tabs::TabGroup;

impl Shell {
    pub(super) fn timeline_header(&self, cx: &mut Context<Self>) -> Option<IslandHeader> {
        let items = tab_items(&self.state.core, self.pending_agent.as_deref());
        if items.is_empty() {
            return None;
        }
        let selected = tabs::view_key(
            self.pending_agent.as_deref(),
            self.state
                .core
                .live_session
                .as_ref()
                .and_then(|session| session.selected_agent.as_deref()),
        )
        .map(str::to_string);
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
            ChromeZones::new(None, None, None).principal(group),
        ))
    }
}
