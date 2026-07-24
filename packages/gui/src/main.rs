//! piko GPUI desktop client entrypoint.
//!
//! Launches DesktopApp with hostd transport.

mod app;
mod bridge;
mod cli;
mod config;
mod features;
mod i18n;
mod projections;
mod shell;
mod theme;
mod transport;

use std::env;

use gpui::*;
use gpui_component::Root;

use crate::app::desktop_app::{
    CancelTurn, CloseTransientOverlay, DesktopApp, FocusComposer, FocusNextIsland, FocusPrevIsland,
    JumpToLatest, NewSession, OpenCommandPalette, OpenSettings, Quit, ToggleRightColumn,
    ToggleSessions,
};
use crate::app::layout_state::{WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH};
use crate::app::quit::is_quit_busy;
use crate::bridge::spawn_bridge;
use crate::features::{
    AgentsConfirm, AgentsSelectNext, AgentsSelectPrev, AgentsToggleExpand, ClearSessionSearch,
    ConfirmSection, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev, SelectNextSection,
    SelectPrevSection, SessionsConfirm, SessionsSelectNext, SessionsSelectPrev,
    SessionsToggleFocused, TreeConfirm, TreeSelectNext, TreeSelectPrev, TreeToggleFocused,
};
use piko_chrome::assets::ChromeAssets;
use piko_chrome::theme::{ChromePalette, apply_chrome_theme};

rust_i18n::i18n!("locales", fallback = "en");

/// Resolve a chrome catalog key (and optional args) to an owned English string.
#[macro_export]
macro_rules! t {
    ($key:expr) => {{
        rust_i18n::t!($key).to_string()
    }};
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        rust_i18n::t!($key, $($name = $value),+).to_string()
    }};
}

fn main() {
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".into());

    let app = Application::new().with_assets(ChromeAssets);

    app.run(move |cx| {
        gpui_component::init(cx);
        piko_chrome::components::init(cx);
        i18n::init();
        // Default dark; hostd [gui].chrome-palette may re-apply after hydrate.
        apply_chrome_theme(cx, ChromePalette::Dark);

        cx.bind_keys([
            KeyBinding::new(
                "cmd-c",
                piko_chrome::components::selection::CopySelection,
                Some("IslandTimeline"),
            ),
            KeyBinding::new("cmd-n", NewSession, Some("DesktopApp")),
            KeyBinding::new("cmd-.", CancelTurn, Some("DesktopApp")),
            KeyBinding::new("cmd-l", FocusComposer, Some("DesktopApp")),
            KeyBinding::new("cmd-j", JumpToLatest, Some("DesktopApp")),
            KeyBinding::new("cmd-b", ToggleSessions, Some("DesktopApp")),
            KeyBinding::new("cmd-i", ToggleRightColumn, Some("DesktopApp")),
            KeyBinding::new("cmd-shift-p", OpenCommandPalette, Some("DesktopApp")),
            KeyBinding::new("cmd-comma", OpenSettings, Some("DesktopApp")),
            KeyBinding::new("escape", CloseTransientOverlay, None),
            KeyBinding::new("escape", ClearSessionSearch, Some("IslandSessionsSearch")),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("tab", FocusNextIsland, Some("DesktopApp")),
            KeyBinding::new("shift-tab", FocusPrevIsland, Some("DesktopApp")),
            KeyBinding::new("up", PaletteSelectPrev, Some("CommandPalette")),
            KeyBinding::new("down", PaletteSelectNext, Some("CommandPalette")),
            KeyBinding::new("enter", PaletteConfirm, Some("CommandPalette")),
            KeyBinding::new("up", SelectPrevSection, Some("SettingsNav")),
            KeyBinding::new("down", SelectNextSection, Some("SettingsNav")),
            KeyBinding::new("enter", ConfirmSection, Some("SettingsNav")),
            KeyBinding::new("space", ConfirmSection, Some("SettingsNav")),
            KeyBinding::new("up", AgentsSelectPrev, Some("IslandAgents")),
            KeyBinding::new("down", AgentsSelectNext, Some("IslandAgents")),
            KeyBinding::new("enter", AgentsConfirm, Some("IslandAgents")),
            KeyBinding::new("space", AgentsConfirm, Some("IslandAgents")),
            KeyBinding::new("right", AgentsToggleExpand, Some("IslandAgents")),
            KeyBinding::new("left", AgentsToggleExpand, Some("IslandAgents")),
            KeyBinding::new("up", SessionsSelectPrev, Some("IslandSessions")),
            KeyBinding::new("down", SessionsSelectNext, Some("IslandSessions")),
            KeyBinding::new("enter", SessionsConfirm, Some("IslandSessions")),
            KeyBinding::new("space", SessionsConfirm, Some("IslandSessions")),
            KeyBinding::new("right", SessionsToggleFocused, Some("IslandSessions")),
            KeyBinding::new("left", SessionsToggleFocused, Some("IslandSessions")),
            KeyBinding::new("up", TreeSelectPrev, Some("IslandTree")),
            KeyBinding::new("down", TreeSelectNext, Some("IslandTree")),
            KeyBinding::new("enter", TreeConfirm, Some("IslandTree")),
            KeyBinding::new("space", TreeConfirm, Some("IslandTree")),
            KeyBinding::new("right", TreeToggleFocused, Some("IslandTree")),
            KeyBinding::new("left", TreeToggleFocused, Some("IslandTree")),
        ]);

        cx.set_menus(vec![Menu {
            name: "piko".into(),
            items: vec![
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        }]);

        // Close last window → quit process (关窗即退出).
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let cwd_clone = cwd.clone();
        cx.spawn(async move |cx| {
            let bridge =
                spawn_bridge(&[], &[("PIKO_LOG_DISABLE", "1")]).expect("failed to spawn hostd");

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(1360.), px(840.)),
                    })),
                    window_min_size: Some(size(px(WINDOW_MIN_WIDTH), px(WINDOW_MIN_HEIGHT))),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(9.), px(10.))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| {
                        let mut app = DesktopApp::new(bridge, cwd_clone, window, cx);
                        app.bootstrap();
                        app
                    });

                    let weak = view.downgrade();
                    window.on_window_should_close(cx, {
                        let weak = weak.clone();
                        move |window, cx| {
                            let Some(entity) = weak.upgrade() else {
                                return true;
                            };
                            let busy = is_quit_busy(entity.read(cx).bridge_state());
                            if busy {
                                entity.update(cx, |app, cx| {
                                    app.request_busy_quit_confirm(window, cx);
                                });
                                false
                            } else {
                                true
                            }
                        }
                    });

                    // Menu / global cmd-q: same busy path as traffic-light close.
                    cx.on_action({
                        let weak = weak.clone();
                        move |_: &Quit, cx: &mut App| {
                            request_quit(&weak, cx);
                        }
                    });

                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}

fn request_quit(weak: &WeakEntity<DesktopApp>, cx: &mut App) {
    let Some(entity) = weak.upgrade() else {
        cx.quit();
        return;
    };
    if !is_quit_busy(entity.read(cx).bridge_state()) {
        cx.quit();
        return;
    }
    let Some(handle) = cx.windows().first().copied() else {
        cx.quit();
        return;
    };
    let _ = handle.update(cx, |_root, window, cx| {
        if let Some(app) = weak.upgrade() {
            app.update(cx, |app, cx| {
                app.request_busy_quit_confirm(window, cx);
            });
        }
    });
}
