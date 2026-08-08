use super::*;

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_composer_restore(window, cx);
        self.sync_layout_breakpoint(window);
        self.sync_selected_agent_scroll(cx);
        self.sync_timeline_follow(cx);
        self.apply_pending_timeline_scroll(cx);
        self.refresh_islands(cx);
        self.sync_prompts(window, cx);
        self.maybe_close_session_sheet_on_live(window, cx);
        self.sync_notifications(window, cx);
        self.maybe_load_draft_on_live(window, cx);

        let on_new = cx.listener(Self::action_new_session);
        let on_cancel = cx.listener(Self::action_cancel_turn);
        let on_focus = cx.listener(Self::action_focus_composer);
        let on_jump = cx.listener(Self::action_jump_to_latest);
        let on_sessions = cx.listener(Self::action_toggle_sessions);
        let on_right_column = cx.listener(Self::action_toggle_right_column);
        let on_focus_next = cx.listener(Self::action_focus_next_island);
        let on_focus_prev = cx.listener(Self::action_focus_prev_island);
        let on_palette = cx.listener(Self::action_open_command_palette);
        let on_close_overlay = cx.listener(Self::action_close_transient_overlay);
        let on_open_settings = cx.listener(Self::action_open_settings);
        let on_toggle_notifications = cx.listener(Self::action_toggle_notification_center);
        let overlay = self.render_active_overlay(window, cx);
        let notification_center = self.render_notification_center_layer(window, cx);
        let toast_layer = self.render_toast_layer(window, cx);

        let mut root = div()
            .id("desktop-app")
            .relative()
            .track_focus(&self.focus_handle)
            .on_action(on_new)
            .on_action(on_cancel)
            .on_action(on_focus)
            .on_action(on_jump)
            .on_action(on_sessions)
            .on_action(on_right_column)
            .on_action(on_focus_next)
            .on_action(on_focus_prev)
            .on_action(on_palette)
            .on_action(on_close_overlay)
            .on_action(on_open_settings)
            .on_action(on_toggle_notifications)
            .key_context("DesktopApp")
            .size_full()
            .flex()
            .flex_col()
            .bg(tokens().canvas_rgba())
            .text_color(tokens().fg_rgba());

        root = match self.archipelago.active() {
            ArchipelagoId::Workbench => mount_workbench_frame(root, self, window, cx),
            ArchipelagoId::Settings => {
                let entity = cx.entity().downgrade();
                let workspace = crate::app::archipelago::settings_workspace();
                mount_settings_frame(
                    root,
                    SettingsFrameChrome {
                        entity,
                        notifications_open: self.notifications.open(),
                        notifications_unread: self.notifications.unread(),
                    },
                    &workspace.island_tree,
                    SettingsIslandId::Nav,
                    SettingsIslandId::Panel,
                    self.settings_nav.clone(),
                    self.settings_panel.clone(),
                )
            }
        };

        root.children(overlay)
            .children(Root::render_sheet_layer(window, cx))
            .children(notification_center)
            .children(toast_layer)
    }
}

impl Focusable for DesktopApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Drop for DesktopApp {
    fn drop(&mut self) {
        self.bridge.shutdown();
    }
}

impl DesktopApp {
    pub(crate) fn action_focus_next_island(
        &mut self,
        _: &FocusNextIsland,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.archipelago.active() {
            ArchipelagoId::Workbench => {
                // Visibility-pruned Tab order; base membership comes from workspace.
                let visible = self.visible_focus_islands();
                self.island_focus.cycle(FocusCycleDir::Next, &visible);
                let id = self.island_focus.focused();
                self.focus_island(id, window, cx);
            }
            ArchipelagoId::Settings => {
                let order = crate::app::archipelago::settings_workspace().focus_order;
                self.settings_focus.cycle(FocusCycleDir::Next, &order);
                let id = self.settings_focus.focused();
                self.focus_settings_island(id, window, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn action_focus_prev_island(
        &mut self,
        _: &FocusPrevIsland,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.archipelago.active() {
            ArchipelagoId::Workbench => {
                let visible = self.visible_focus_islands();
                self.island_focus.cycle(FocusCycleDir::Prev, &visible);
                let id = self.island_focus.focused();
                self.focus_island(id, window, cx);
            }
            ArchipelagoId::Settings => {
                let order = crate::app::archipelago::settings_workspace().focus_order;
                self.settings_focus.cycle(FocusCycleDir::Prev, &order);
                let id = self.settings_focus.focused();
                self.focus_settings_island(id, window, cx);
            }
        }
        cx.notify();
    }
}
