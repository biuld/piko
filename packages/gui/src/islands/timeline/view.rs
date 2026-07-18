//! Timeline island: Entity-owned conversation document + scroll state.

use std::collections::HashSet;

use gpui::*;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::chrome::{IslandId, IslandMsg, IslandPanel, IslandPlaceholder};

use super::render::render_timeline_body;
use super::vm::TimelineViewModel;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

pub struct TimelineIsland {
    focus_handle: FocusHandle,
    host: WeakEntity<DesktopApp>,
    chrome_focused: bool,
    vm: TimelineViewModel,
    follow: bool,
    expanded_tools: HashSet<String>,
    scroll: ScrollHandle,
}

impl TimelineIsland {
    pub fn new(host: WeakEntity<DesktopApp>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            host,
            chrome_focused: false,
            vm: TimelineViewModel::default(),
            follow: false,
            expanded_tools: HashSet::new(),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn apply_timeline(&mut self, vm: TimelineViewModel, cx: &mut Context<Self>) {
        self.vm = vm;
        cx.notify();
    }

    pub fn set_follow(&mut self, follow: bool, cx: &mut Context<Self>) {
        if self.follow != follow {
            self.follow = follow;
            cx.notify();
        }
    }

    pub fn toggle_tool(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if !self.expanded_tools.remove(row_id) {
            self.expanded_tools.insert(row_id.to_string());
        }
        cx.notify();
    }

    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    pub fn scroll_to_item(&self, ix: usize) {
        self.scroll.scroll_to_item(ix);
    }

    pub fn scroll_to_bottom(&self) {
        self.scroll.scroll_to_bottom();
    }

    pub fn set_offset(&self, offset: Point<Pixels>) {
        self.scroll.set_offset(offset);
    }

    pub fn offset(&self) -> Point<Pixels> {
        self.scroll.offset()
    }

    pub fn set_chrome_focused(&mut self, focused: bool, cx: &mut Context<Self>) {
        if self.chrome_focused != focused {
            self.chrome_focused = focused;
            cx.notify();
        }
    }

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Timeline, msg, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }
}

impl Focusable for TimelineIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TimelineIsland {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();

        let on_toggle_tool = move |row_id: String| -> ClickHandler {
            let entity = entity.clone();
            Box::new(move |_, window, cx| {
                if let Some(view) = entity.upgrade() {
                    view.update(cx, |this, cx| {
                        this.toggle_tool(&row_id, cx);
                        this.emit(
                            IslandMsg::ToggleToolDetail {
                                row_id: row_id.clone(),
                            },
                            window,
                            cx,
                        );
                    });
                }
            })
        };

        let panel = if self.vm.rows.is_empty() {
            IslandPanel::empty(
                "timeline-island",
                IslandPlaceholder::new("No messages yet")
                    .icon("✍")
                    .subtitle("Send a message to start the conversation"),
            )
            .focused(self.chrome_focused)
            .into_any_element()
        } else {
            IslandPanel::new(
                "timeline-island",
                render_timeline_body(&self.vm, &self.expanded_tools, on_toggle_tool, window, cx),
            )
            .scroll_handle(self.scroll.clone())
            .focused(self.chrome_focused)
            .into_any_element()
        };

        div()
            .id("timeline-island-root")
            .track_focus(&self.focus_handle)
            .key_context("IslandTimeline")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::claim_focus))
            .child(panel)
    }
}
