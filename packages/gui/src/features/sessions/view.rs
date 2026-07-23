//! Sessions island: Entity-owned session sidebar.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use piko_chrome::{ListKeyEffect, ListKeyIntent, ListKeyboard};

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::projections::{SidebarViewModel, apply_sidebar_search};
use crate::shell::{IslandId, IslandMsg, IslandView, activate_focus_handle};

use super::sidebar::{
    ClickHandler, IdClickFactory, SearchFocusHandler, SessionListTarget, SessionMenuFactory,
    SidebarPanelHandlers, build_session_context_menu, render_sidebar_panel,
    visible_session_targets,
};

actions!(
    sessions,
    [
        ClearSessionSearch,
        SessionsSelectPrev,
        SessionsSelectNext,
        SessionsConfirm,
        SessionsToggleFocused
    ]
);

pub struct SessionsIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: SidebarViewModel,
    search_input: Entity<InputState>,
    list_scroll: ScrollHandle,
    collapsed_dirs: HashSet<String>,
    list_kb: ListKeyboard,
}

impl SessionsIsland {
    pub fn new(host: WeakEntity<DesktopApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::t!("island.sessions.search.placeholder"))
        });
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: SidebarViewModel {
                pinned: Vec::new(),
                groups: Vec::new(),
            },
            search_input,
            list_scroll: ScrollHandle::new(),
            collapsed_dirs: HashSet::new(),
            list_kb: ListKeyboard::new(),
        }
    }

    pub fn subscribe_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(
            &self.search_input,
            window,
            |_this, _input, _event: &InputEvent, _window, cx| {
                cx.notify();
            },
        )
        .detach();
    }

    pub fn apply(&mut self, vm: SidebarViewModel, cx: &mut Context<Self>) {
        self.vm = vm;
        let filtered = self.filtered_vm(cx);
        self.list_kb
            .sync_len(visible_session_targets(&filtered, &self.collapsed_dirs).len());
        cx.notify();
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

    fn filtered_vm(&self, cx: &App) -> SidebarViewModel {
        let query = self.search_input.read(cx).value().to_string();
        apply_sidebar_search(&self.vm, &query)
    }

    fn select_prev(&mut self, _: &SessionsSelectPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Prev, window, cx);
    }

    fn select_next(&mut self, _: &SessionsSelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Next, window, cx);
    }

    fn confirm(&mut self, _: &SessionsConfirm, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_list_key(ListKeyIntent::Activate, window, cx);
    }

    fn toggle_focused(
        &mut self,
        _: &SessionsToggleFocused,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_list_key(ListKeyIntent::ToggleExpand, window, cx);
    }

    fn apply_list_key(
        &mut self,
        intent: ListKeyIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filtered = self.filtered_vm(cx);
        let targets = visible_session_targets(&filtered, &self.collapsed_dirs);
        match self.list_kb.apply(targets.len(), intent) {
            ListKeyEffect::None => {}
            ListKeyEffect::CursorMoved { .. } => cx.notify(),
            ListKeyEffect::Activate { index } => {
                if let Some(target) = targets.get(index) {
                    match target {
                        SessionListTarget::Directory(key) => self.toggle_dir(key.clone(), cx),
                        SessionListTarget::Session(session_id) => self.emit(
                            IslandMsg::OpenSession {
                                session_id: session_id.clone(),
                            },
                            window,
                            cx,
                        ),
                    }
                }
                cx.notify();
            }
            ListKeyEffect::ToggleExpand { index } => {
                if let Some(SessionListTarget::Directory(key)) = targets.get(index) {
                    self.toggle_dir(key.clone(), cx);
                } else {
                    cx.notify();
                }
            }
        }
    }

    /// Focus the filter field and claim Sessions chrome ownership.
    ///
    /// Emits [`IslandMsg::ClaimFocus`] (chrome-only / Claimed). Does not go
    /// through `focus_island`, which would Activate and steal from `InputState`.
    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.emit(IslandMsg::ClaimFocus, window, cx);
        self.search_input.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    fn clear_search(
        &mut self,
        _: &ClearSessionSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let empty = self.search_input.read(cx).value().is_empty();
        if empty {
            return;
        }
        self.search_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        cx.notify();
    }
}

impl IslandView for SessionsIsland {
    type Id = IslandId;

    fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            if focused {
                let filtered = self.filtered_vm(cx);
                let targets = visible_session_targets(&filtered, &self.collapsed_dirs);
                self.list_kb.ensure_cursor(targets.len(), None);
            }
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

impl Focusable for SessionsIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionsIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let vm = self.filtered_vm(cx);
        let has_sessions =
            !self.vm.pinned.is_empty() || self.vm.groups.iter().any(|group| !group.rows.is_empty());

        let on_open_directory: ClickHandler = Box::new(cx.listener(Self::on_open_directory));

        let on_search_focus: SearchFocusHandler = Rc::new({
            let entity = entity.clone();
            move |window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.focus_search(window, cx);
                    });
                }
            }
        });

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

        let session_menu: SessionMenuFactory = Box::new({
            let entity = entity.clone();
            move |row| {
                let entity = entity.clone();
                let session_id = row.session_id.clone();
                build_session_context_menu(
                    row,
                    Rc::new({
                        let entity = entity.clone();
                        let session_id = session_id.clone();
                        move |window, cx| {
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
                        }
                    }),
                    Rc::new({
                        let entity = entity.clone();
                        let session_id = session_id.clone();
                        move |window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.emit(
                                        IslandMsg::RenameSession {
                                            session_id: session_id.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                    }),
                    Rc::new({
                        let entity = entity.clone();
                        let session_id = session_id.clone();
                        move |window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.emit(
                                        IslandMsg::TogglePinSession {
                                            session_id: session_id.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                    }),
                    Rc::new({
                        let entity = entity.clone();
                        let session_id = session_id.clone();
                        move |window, cx| {
                            if let Some(view) = entity.upgrade() {
                                view.update(cx, |this, cx| {
                                    this.emit(
                                        IslandMsg::DeleteSession {
                                            session_id: session_id.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                });
                            }
                        }
                    }),
                )
            }
        });

        let handlers = SidebarPanelHandlers {
            on_open_session: &on_open,
            on_new_session: &on_new,
            on_toggle_dir: &on_toggle_dir,
            session_menu: &session_menu,
            on_search_focus: &on_search_focus,
        };
        let panel = render_sidebar_panel(
            &vm,
            has_sessions,
            &self.collapsed_dirs,
            self.search_input.clone(),
            self.list_scroll.clone(),
            on_open_directory,
            handlers,
            self.chrome_focused,
            self.list_kb.cursor().filter(|_| self.chrome_focused),
        );

        div()
            .id("sessions-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandSessions")
            .on_action(cx.listener(Self::clear_search))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_focused))
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
