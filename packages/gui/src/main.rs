//! piko GPUI desktop client entrypoint.
//!
//! Launches DesktopApp with hostd transport.

mod app;
mod bridge;
mod chrome;
mod cli;
mod config;
mod islands;
mod overlays;
mod projections;
mod theme;
mod transport;

use std::env;

use gpui::*;
use gpui_component::Root;

use crate::app::desktop_app::{
    CancelTurn, DesktopApp, FocusComposer, FocusNextIsland, FocusPrevIsland, JumpToLatest,
    NewSession, ToggleAgentsTree, ToggleSessions,
};
use crate::bridge::spawn_bridge;
use crate::theme::apply_piko_dark_theme;

fn main() {
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".".into());

    let app = Application::new();

    app.run(move |cx| {
        gpui_component::init(cx);
        apply_piko_dark_theme(cx);

        cx.bind_keys([
            KeyBinding::new("cmd-n", NewSession, Some("DesktopApp")),
            KeyBinding::new("cmd-.", CancelTurn, Some("DesktopApp")),
            KeyBinding::new("cmd-l", FocusComposer, Some("DesktopApp")),
            KeyBinding::new("cmd-j", JumpToLatest, Some("DesktopApp")),
            KeyBinding::new("cmd-b", ToggleSessions, Some("DesktopApp")),
            KeyBinding::new("cmd-i", ToggleAgentsTree, Some("DesktopApp")),
            KeyBinding::new("tab", FocusNextIsland, Some("DesktopApp")),
            KeyBinding::new("shift-tab", FocusPrevIsland, Some("DesktopApp")),
        ]);

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
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
