//! Sessions island: Entity-owned session sidebar.
//!
use std::collections::HashSet;

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::projections::SidebarViewModel;
use crate::shell::{IslandId, IslandMsg};

use super::sidebar::{ClickHandler, IdClickFactory, render_sidebar_panel};

pub struct SessionsIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: SidebarViewModel,
    /// Directory cwd keys that are collapsed (absent ⇒ expanded).
    collapsed_dirs: HashSet<String>,
}

impl SessionsIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: SidebarViewModel { groups: Vec::new() },
            collapsed_dirs: HashSet::new(),
        }
    }

    /// Push a freshly derived sidebar projection (host owns derivation).
    pub fn apply(&mut self, vm: SidebarViewModel, cx: &mut Context<Self>) {
        self.vm = vm;
        cx.notify();
    }

    /// Update the chrome-owned focus ring flag (draws the ring border).
    pub fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Sessions, msg, window, cx);
    }

    fn on_open_directory(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.emit(IslandMsg::OpenDirectory, window, cx);
    }

    fn toggle_dir(&mut self, key: String, cx: &mut Context<Self>) {
        if !self.collapsed_dirs.remove(&key) {
            self.collapsed_dirs.insert(key);
        }
        cx.notify();
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }
}

impl Focusable for SessionsIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionsIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();

        let on_open_directory: ClickHandler = Box::new(cx.listener(Self::on_open_directory));

        let on_open: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |session_id| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.emit(
                                IslandMsg::OpenSession {
                                    session_id: session_id.clone(),
                                },
                                window,
                                cx,
                            );
                        });
                    }
                })
            }
        });

        let on_new: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |cwd| {
                let entity = entity.clone();
                Box::new(move |_, window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.emit(IslandMsg::NewSession { cwd: cwd.clone() }, window, cx);
                        });
                    }
                })
            }
        });

        let on_toggle_dir: IdClickFactory = Box::new({
            let entity = entity.clone();
            move |dir_key| {
                let entity = entity.clone();
                Box::new(move |_, _window, cx| {
                    if let Some(view) = entity.upgrade() {
                        view.update(cx, |this, cx| {
                            this.toggle_dir(dir_key.clone(), cx);
                        });
                    }
                })
            }
        });

        let panel = render_sidebar_panel(
            &self.vm,
            &self.collapsed_dirs,
            on_open_directory,
            on_open,
            on_new,
            on_toggle_dir,
            self.chrome_focused,
        );

        div()
            .id("sessions-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandSessions")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
