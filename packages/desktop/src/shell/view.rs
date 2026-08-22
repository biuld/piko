use super::rows::render_row;
use super::*;
use island::components::panel::{
    IslandPanel, IslandPlaceholder, PanelPresentation, PanelSurfaceRole,
};
use island::theme::IslandIcon;

/// Device-dependent wheel rounding still considered the Timeline tail.
const FOLLOW_EPSILON: f32 = 4.0;

fn is_at_tail(offset_y: f32, max_offset_y: f32) -> bool {
    max_offset_y <= FOLLOW_EPSILON || (max_offset_y + offset_y).abs() <= FOLLOW_EPSILON
}

impl Shell {
    fn sync_follow(&mut self) {
        let max = self.scroll.max_offset().y;
        let y = self.scroll.offset().y;
        let at_bottom = is_at_tail(f32::from(y), f32::from(max));
        if self.wheel_seen && !at_bottom {
            self.view_local().following = false;
        } else if at_bottom {
            self.view_local().following = true;
        }
        self.wheel_seen = false;
        let following = self.following();
        if following && self.state.connection == DesktopConnection::Live {
            self.scroll.scroll_to_bottom();
        }
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reconcile_draft_target(window, cx);
        self.persist_presentation(window);
        let t = tokens();
        let width = window.bounds().size.width;
        let persistent_sidebar =
            sidebar::uses_persistent_sidebar(f32::from(width), self.prefs.sidebar_collapsed);
        let narrow = width < px(sidebar::SIDEBAR_WIDTH + sidebar::MIN_TIMELINE_WIDTH);

        let on_activate = {
            let entity = cx.entity().downgrade();
            std::rc::Rc::new(
                move |id: sidebar::NavId, window: &mut Window, app: &mut App| {
                    if let Some(shell) = entity.upgrade() {
                        shell.update(app, |shell, cx| shell.activate_nav(id, window, cx));
                    }
                },
            )
        };

        self.sync_follow();
        let timeline_state = if self.pending_agent.is_some() {
            timeline::TimelineState::Loading
        } else if let Some(error) = self.selection_error.clone() {
            timeline::TimelineState::Error(error)
        } else {
            timeline::timeline_state(&self.state.core)
        };

        use island::components::chrome::ChromeZones;
        use island::components::workspace::{
            WindowChromeFrame, WorkspaceChrome, WorkspacePresentation,
        };

        let content = self.render_timeline_region(timeline_state, cx);
        let mut frame = WindowChromeFrame::new(content, self.render_chrome(cx, narrow))
            .presentation(WorkspacePresentation::Detached)
            .material(self.material);
        if persistent_sidebar {
            frame = frame.sidebar(
                sidebar::render_sidebar_content(
                    sidebar::nav_model(&self.state.core, self.sidebar_keyboard_focused),
                    &self.sidebar_scroll,
                    on_activate.clone(),
                ),
                WorkspaceChrome::new(ChromeZones::leading(self.sidebar_toggle_icon(cx, false)))
                    .surface_role(SurfaceRole::Sidebar)
                    .material(self.material),
                px(sidebar::SIDEBAR_WIDTH),
            );
        }

        div()
            .id("piko-shell")
            .size_full()
            .relative()
            .bg(fill(SurfaceRole::Canvas, self.material))
            .text_color(t.fg_rgba())
            .child(frame)
            .when(!persistent_sidebar && self.narrow_overlay_open, |root| {
                root.child(
                    div()
                        .absolute()
                        .left_2()
                        .top_2()
                        .bottom_2()
                        .w(px(sidebar::SIDEBAR_WIDTH))
                        .child(sidebar::render_sidebar_surface(
                            sidebar::nav_model(&self.state.core, self.sidebar_keyboard_focused),
                            self.material,
                            &self.sidebar_scroll,
                            on_activate,
                        )),
                )
            })
            .when_some(self.layers.active(), |root, layer| {
                root.child(self.render_temporary_layer(layer, window, cx))
            })
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                this.handle_shell_key(event, narrow, window, cx);
            }))
    }
}

impl Shell {
    fn toggle_sidebar(&mut self, narrow: bool, cx: &mut Context<Self>) {
        if narrow {
            self.narrow_overlay_open = !self.narrow_overlay_open;
        } else {
            self.prefs.sidebar_collapsed = !self.prefs.sidebar_collapsed;
            self.narrow_overlay_open = false;
            let _ = self.prefs.save(&self.prefs_path);
        }
        cx.notify();
    }

    fn sidebar_toggle_icon(&self, cx: &mut Context<Self>, show: bool) -> AnyElement {
        use island::components::chrome::GhostIconButton;
        let icon = if show {
            IslandIcon::PanelLeft
        } else {
            IslandIcon::PanelLeftFilled
        };
        let tooltip = if show { "Show Sidebar" } else { "Hide Sidebar" };
        let t = tokens();
        GhostIconButton::new("piko-sessions-toggle", icon, t.fg_rgba())
            .tooltip(tooltip)
            .material(self.material)
            .on_click(cx.listener(move |this, _, window, cx| {
                let narrow = f32::from(window.bounds().size.width)
                    < sidebar::SIDEBAR_WIDTH + sidebar::MIN_TIMELINE_WIDTH;
                this.toggle_sidebar(narrow, cx);
            }))
            .into_any_element()
    }

    fn render_chrome(
        &self,
        cx: &mut Context<Self>,
        narrow: bool,
    ) -> island::components::workspace::WorkspaceChrome {
        use island::components::chrome::ChromeZones;
        use island::components::workspace::WorkspaceChrome;

        let mut zones = ChromeZones::new(None, Some(self.workspace_toolbar(cx)), None);
        if let Some(tabs) = self.agent_tab_group(cx) {
            zones = zones.principal(tabs).principal_min_width(px(180.));
        }
        if narrow || self.prefs.sidebar_collapsed {
            zones = zones.prepend_leading(self.sidebar_toggle_icon(cx, true));
        }

        WorkspaceChrome::new(zones).material(self.material)
    }

    fn render_timeline_region(
        &mut self,
        state: timeline::TimelineState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = metrics();
        let input = self.composer_input.read(cx);
        let draft = input.value().to_string();
        let measured_input_height = f32::from(input.input_bounds().size.height);
        let following = self.following();
        let composer_padding =
            composer::footprint_for_text(&draft, measured_input_height, !following);

        let panel = match &state {
            timeline::TimelineState::NoSession => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("No session selected")
                    .chrome_icon(IslandIcon::MessageSquare)
                    .subtitle("Pick a session in the sidebar to open its conversation."),
            ),
            timeline::TimelineState::Loading => IslandPanel::loading(
                "piko-timeline",
                IslandPlaceholder::new("Loading")
                    .chrome_icon(IslandIcon::CircleDashed)
                    .subtitle("Opening the selected session…"),
            ),
            timeline::TimelineState::Error(error) => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("Could not load conversation")
                    .chrome_icon(IslandIcon::TriangleAlert)
                    .subtitle(error.clone()),
            ),
            timeline::TimelineState::Empty => IslandPanel::empty(
                "piko-timeline",
                IslandPlaceholder::new("No messages yet")
                    .chrome_icon(IslandIcon::MessageSquare)
                    .subtitle("Start the conversation from the composer."),
            ),
            timeline::TimelineState::Ready(rows) => IslandPanel::new(
                "piko-timeline",
                div()
                    .id("piko-timeline-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.set_focus_owner(FocusOwner::Timeline, window, cx);
                        }),
                    )
                    .on_scroll_wheel(cx.listener(move |this, _, window, cx| {
                        this.wheel_seen = true;
                        this.set_focus_owner(FocusOwner::Timeline, window, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(m.space_md)
                            .px(m.space_lg)
                            .py(m.space_lg)
                            .pb(px(composer_padding))
                            .children(rows.iter().map(|row| render_row(row, &self.material))),
                    ),
            )
            .scroll(false),
        }
        .material(self.material)
        .surface_role(PanelSurfaceRole::Content)
        .presentation(PanelPresentation::Detached);

        let show_return = matches!(&state, timeline::TimelineState::Ready(rows) if !rows.is_empty())
            && !following;

        // Fill the WindowChromeFrame content slot. `flex_1` here collapses
        // because that slot is not a flex column; a zero-height parent then
        // pins the absolutely positioned Composer into the title band.
        div()
            .id("piko-timeline-region")
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .relative()
            // Meet the toolbar tab notch at the chrome baseline.
            .mt(-metrics().island_gutter)
            .child(panel)
            .when(show_return, |region| {
                region.child(
                    div()
                        .id("piko-return-to-latest")
                        .absolute()
                        .bottom(px(composer_padding - 18.0))
                        .left(px(12.))
                        .right(px(12.))
                        .px(m.space_sm)
                        .py(px(2.))
                        .rounded_sm()
                        .border_1()
                        .border_color(hairline(SurfaceRole::Chrome))
                        .bg(fill(SurfaceRole::Elevated, self.material))
                        .cursor_pointer()
                        .hover(|style| style.bg(highlight()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.view_local().following = true;
                            this.scroll.scroll_to_bottom();
                            cx.notify();
                        }))
                        .child(text(TextRole::Meta).child("↓ Latest")),
                )
            })
            .child(self.render_composer(cx))
            .into_any_element()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let live = self.state.connection == DesktopConnection::Live
            && self.state.core.session_phase == SessionPhase::Live;
        let view_key = self.view_key();
        let running = self
            .state
            .core
            .live_session
            .as_ref()
            .is_some_and(|session| {
                session
                    .active_turns
                    .iter()
                    .any(|turn| Some(turn.agent_instance_id.as_str()) == view_key)
            });
        let context = self
            .state
            .core
            .live_session
            .as_ref()
            .and_then(|session| session.last_context_tokens)
            .zip(self.state.core.model.active_context_window())
            .map(|(used, window)| composer::ComposerContext::Fill { used, window })
            .unwrap_or(composer::ComposerContext::Unknown);
        let entity = cx.entity().downgrade();
        let submit = std::rc::Rc::new(move |window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| shell.submit_composer(window, cx));
            }
        });
        let entity = cx.entity().downgrade();
        let cancel = std::rc::Rc::new(move |_window: &mut Window, app: &mut App| {
            if let Some(shell) = entity.upgrade() {
                shell.update(app, |shell, cx| shell.cancel_turn(cx));
            }
        });

        composer::ComposerView {
            input: self.composer_input.clone(),
            material: self.material,
            enabled: live && view_key.is_some() && self.pending_agent.is_none(),
            running,
            pending: self.pending_submission().is_some(),
            context,
            error: self.composer_error(),
            on_submit: submit,
            on_cancel: cancel,
        }
        .render()
    }
}

#[cfg(test)]
mod tests {
    use super::is_at_tail;

    #[test]
    fn gpui_negative_scroll_offsets_detect_the_tail() {
        assert!(is_at_tail(-100.0, 100.0));
        assert!(is_at_tail(-97.0, 100.0));
        assert!(!is_at_tail(-70.0, 100.0));
        assert!(is_at_tail(0.0, 0.0));
    }
}
