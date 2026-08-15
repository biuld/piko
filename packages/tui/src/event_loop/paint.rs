use std::io::Stdout;

use anyhow::{Context, Result};
use crossterm::SynchronizedUpdate;
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{app::AppState, layout, render};

pub(super) fn paint(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut AppState,
) -> Result<layout::PreparedFrame> {
    let size = terminal.size().context("read terminal size")?;
    let terminal_rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let mut prepared = layout::prepare_frame(app, terminal_rect);
    std::io::stdout()
        .sync_update(|_| terminal.draw(|frame| render::render_prepared(frame, app, &mut prepared)))
        .context("sync update terminal")?
        .context("draw terminal")?;
    Ok(prepared)
}
