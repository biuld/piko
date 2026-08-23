mod cli;
mod connection;
mod focus;
mod prefs;
mod shell;
mod state;
mod transport;

use anyhow::Result;
use gpui::prelude::*;
use gpui::{Bounds, WindowBounds, px, size};

fn main() -> Result<()> {
    let args = cli::CliArgs::parse();
    let prefs_path = prefs::default_path();
    let prefs = prefs::DesktopPrefs::load(&prefs_path);

    island::platform::application()
        .with_assets(island::assets::IslandAssets)
        .run(move |cx| {
            island::components::init(cx);
            island::theme::apply(cx, island::theme::IslandPalette::Dark);

            let restored = prefs.window.map(prefs::WindowRect::into_bounds);
            let bounds = restored
                .filter(|bounds| {
                    cx.displays().iter().any(|display| {
                        let visible = bounds.intersect(&display.bounds());
                        visible.size.width >= px(160.) && visible.size.height >= px(80.)
                    })
                })
                .unwrap_or_else(|| Bounds::centered(None, size(px(1180.), px(780.)), cx));
            let mut options = island::platform::window::window_options();
            options.window_bounds = Some(WindowBounds::Windowed(bounds));
            cx.open_window(options, |window, cx| {
                // gpui-base inputs render at `rem(1.0)`; align the default rem to
                // island's body size so input typography matches the theme.
                window.set_rem_size(island::theme::metrics().body_size);
                cx.new(|cx| {
                    shell::Shell::new(window, cx, args.clone(), prefs_path.clone(), prefs.clone())
                })
            })
            .expect("open piko window");
            cx.activate(true);
        });

    Ok(())
}
