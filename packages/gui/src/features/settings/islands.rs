//! Settings archipelago islands: Nav + Panel as real [`IslandView`] entities.
//!
//! Layout tree: [`crate::app::archipelago::settings_workspace`].
//! Nav list cursor: [`ListKeyboard`] only (roadmap D5).

use gpui::*;
use piko_chrome::{ListKeyEffect, ListKeyIntent, ListKeyboard};

use crate::app::desktop_app::DesktopApp;
use crate::shell::{IslandHeader, IslandPanel, IslandView, activate_focus_handle};

use super::SettingsSection;
use super::nav::{ConfirmSection, SelectNextSection, SelectPrevSection, render_nav};
use super::render::render_panel;

/// Leaf ids inside the Settings archipelago body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsIslandId {
    Nav,
    Panel,
}

pub const SETTINGS_FOCUS_ORDER: [SettingsIslandId; 2] =
    [SettingsIslandId::Nav, SettingsIslandId::Panel];

// ── Nav ────────────────────────────────────────────────────────────────────

pub struct SettingsNavIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    section: SettingsSection,
    list_kb: ListKeyboard,
}

impl SettingsNavIsland {
    pub fn new(
        host: WeakEntity<DesktopApp>,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut list_kb = ListKeyboard::new();
        list_kb.set_cursor(SettingsSection::ALL.len(), section_index(section));
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            section,
            list_kb,
        }
    }

    pub fn apply(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        if self.section != section {
            self.section = section;
            self.list_kb
                .set_cursor(SettingsSection::ALL.len(), section_index(section));
            cx.notify();
        }
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        let host = self.host.clone();
        window.defer(cx, move |window, cx| {
            if let Some(host) = host.upgrade() {
                host.update(cx, |app, cx| {
                    app.claim_settings_island_focus(SettingsIslandId::Nav, window, cx);
                });
            }
        });
    }

    fn select_prev(&mut self, _: &SelectPrevSection, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Prev, window, cx);
    }

    fn select_next(&mut self, _: &SelectNextSection, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Next, window, cx);
    }

    fn confirm_section(&mut self, _: &ConfirmSection, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Activate, window, cx);
    }

    fn apply_list_key(
        &mut self,
        intent: ListKeyIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let len = SettingsSection::ALL.len();
        match self.list_kb.apply(len, intent) {
            ListKeyEffect::None => {}
            ListKeyEffect::CursorMoved { index } => {
                let next = SettingsSection::ALL[index];
                self.section = next;
                cx.notify();
                // Must not call host → settings_nav.update while this island is
                // still inside its own Entity::update (keyboard action) — GPUI
                // panics on re-entrant updates. Defer like claim_focus / island msgs.
                let host = self.host.clone();
                window.defer(cx, move |_window, cx| {
                    if let Some(host) = host.upgrade() {
                        host.update(cx, |app, cx| {
                            app.select_settings_section(next, cx);
                        });
                    }
                });
            }
            ListKeyEffect::Activate { index } => {
                let section = SettingsSection::ALL[index];
                self.section = section;
                cx.notify();
                let host = self.host.clone();
                window.defer(cx, move |window, cx| {
                    if let Some(host) = host.upgrade() {
                        host.update(cx, |app, cx| {
                            app.select_settings_section(section, cx);
                            app.focus_settings_island(SettingsIslandId::Panel, window, cx);
                        });
                    }
                });
            }
            ListKeyEffect::ToggleExpand { .. } => {
                // Flat nav — no expand.
            }
        }
    }
}

fn section_index(section: SettingsSection) -> usize {
    SettingsSection::ALL
        .iter()
        .position(|s| *s == section)
        .unwrap_or(0)
}

impl IslandView for SettingsNavIsland {
    type Id = SettingsIslandId;

    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    fn take_keyboard_focus(
        &mut self,
        reason: crate::shell::FocusReason,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        activate_focus_handle(&self.focus_handle, reason, window);
    }
}

impl Focusable for SettingsNavIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsNavIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host = self.host.clone();
        let panel = IslandPanel::new(
            "settings-nav-island",
            render_nav(self.section, self.chrome_focused, host),
        )
        .header(IslandHeader::title(crate::t!("settings.title")))
        .scroll(false)
        .focused(self.chrome_focused);

        div()
            .id("settings-nav-island-wrap")
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("SettingsNav")
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::confirm_section))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}

// ── Panel ──────────────────────────────────────────────────────────────────

pub struct SettingsPanelIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    section: SettingsSection,
}

impl SettingsPanelIsland {
    pub fn new(
        host: WeakEntity<DesktopApp>,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            section,
        }
    }

    pub fn apply(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        if self.section != section {
            self.section = section;
            cx.notify();
        }
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        let host = self.host.clone();
        window.defer(cx, move |window, cx| {
            if let Some(host) = host.upgrade() {
                host.update(cx, |app, cx| {
                    app.claim_settings_island_focus(SettingsIslandId::Panel, window, cx);
                });
            }
        });
    }
}

impl IslandView for SettingsPanelIsland {
    type Id = SettingsIslandId;

    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    fn take_keyboard_focus(
        &mut self,
        reason: crate::shell::FocusReason,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        activate_focus_handle(&self.focus_handle, reason, window);
    }
}

impl Focusable for SettingsPanelIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsPanelIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let section = self.section;
        let host = self.host.clone();
        let body = match host.upgrade() {
            Some(app) => {
                let app_ref = app.read(cx);
                render_panel(section, app_ref, host).into_any_element()
            }
            None => div().into_any_element(),
        };

        let panel = IslandPanel::new("settings-panel-island", body)
            .scroll(false)
            .focused(self.chrome_focused);

        div()
            .id("settings-panel-island-wrap")
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("SettingsPanel")
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
