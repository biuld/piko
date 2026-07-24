//! Timeline island: Entity-owned conversation document + scroll state.

use std::collections::HashSet;

use gpui::*;
use piko_chrome::components::selection::CopySelection;

use crate::app::desktop_app::DesktopApp;
use crate::app::island_dispatch::schedule_island_msg;
use crate::shell::{
    IslandId, IslandMsg, IslandPanel, IslandPlaceholder, IslandView, activate_focus_handle,
};

use super::markdown_cache::TimelineMarkdownCache;
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
    markdown: TimelineMarkdownCache,
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
            markdown: TimelineMarkdownCache::new(cx.entity_id()),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn apply_timeline(&mut self, vm: TimelineViewModel, cx: &mut Context<Self>) {
        self.markdown.sync(&vm.rows, cx);
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

    fn emit(&self, msg: IslandMsg, window: &mut Window, cx: &mut Context<Self>) {
        schedule_island_msg(self.host.clone(), IslandId::Timeline, msg, window, cx);
    }

    fn claim_focus(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.emit(IslandMsg::ClaimFocus, window, cx);
    }

    fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.markdown.selected_text(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

impl IslandView for TimelineIsland {
    type Id = IslandId;

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

impl Focusable for TimelineIsland {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TimelineIsland {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        let allow_motion = self
            .host
            .upgrade()
            .map(|host| host.read(cx).ux_prefs.allow_motion())
            .unwrap_or(true);

        let panel = if self.vm.rows.is_empty() {
            IslandPanel::empty(
                "timeline-island",
                IslandPlaceholder::new(crate::t!("island.timeline.empty.title"))
                    .chrome_icon(crate::theme::ChromeIcon::MessageSquare)
                    .subtitle(crate::t!("island.timeline.empty.subtitle")),
            )
            .focused(self.chrome_focused)
            .into_any_element()
        } else {
            IslandPanel::new(
                "timeline-island",
                render_timeline_body(
                    &self.vm,
                    &self.markdown,
                    &self.expanded_tools,
                    allow_motion,
                    on_toggle_tool,
                    cx,
                ),
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
            .on_action(cx.listener(Self::copy_selection))
            .child(panel)
    }
}
