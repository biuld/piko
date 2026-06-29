//! BottomBar — always-visible status row at the bottom of the TUI.
//!
//! Displays contextual session information as items separated by `·`.
//! No key hints or interactive prompts.  Configurable via `tui.bottomBar.*` settings.
//!
//! Default items: model · cwd · context · cost

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::AppState, config::bottom_bar::BottomBarItem};

pub struct BottomBar;

impl BottomBar {
    pub fn render(frame: &mut Frame<'_>, area: Rect, app: &AppState) {
        let items: Vec<Span<'_>> = app
            .tui_config
            .bottom_bar
            .items
            .iter()
            .flat_map(|item| {
                let span = match item {
                    BottomBarItem::Model => render_model(app),
                    BottomBarItem::Cwd => render_cwd(app),
                    BottomBarItem::Context => render_context(app),
                    BottomBarItem::Cost => render_cost(app),
                };
                // Insert separator between items
                [
                    Span::raw(" "),
                    separator(app.theme.dim),
                    Span::raw(" "),
                    span,
                ]
            })
            .collect();

        // Drop the leading separator
        let items = if items.len() >= 3 {
            &items[3..] // skip first " · "
        } else {
            &items[..]
        };

        let line = Line::from(items.to_vec());
        let paragraph = Paragraph::new(line).style(Style::default().fg(app.theme.muted));
        frame.render_widget(paragraph, area);
    }
}

// ── separator ────────────────────────────────────────────────────────────────

fn separator(dim: ratatui::style::Color) -> Span<'static> {
    Span::styled("·", Style::default().fg(dim))
}

// ── item renderers ───────────────────────────────────────────────────────────

fn render_model(app: &AppState) -> Span<'_> {
    // Try to get the active model from the status string (hostd includes it on model change).
    // Fall back to initial_options.
    let model = app.initial_options.model_id.as_deref().unwrap_or("—");

    let thinking = app
        .initial_options
        .thinking_level
        .as_deref()
        .unwrap_or("off");

    let text = if thinking == "off" {
        model.to_string()
    } else {
        format!("{model} {thinking}")
    };

    Span::raw(text)
}

fn render_cwd(app: &AppState) -> Span<'_> {
    let cwd_binding = app.cwd();
    let cwd_str = cwd_binding.to_string_lossy();

    // Replace $HOME with ~
    let display = if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if cwd_str.starts_with(home_str.as_ref()) {
            let relative = &cwd_str[home_str.len()..];
            if relative.is_empty() {
                "~".to_string()
            } else {
                format!("~{relative}")
            }
        } else {
            cwd_str.to_string()
        }
    } else {
        cwd_str.to_string()
    };

    // Truncate from left if too long
    let display = if display.len() > 40 {
        format!("…{}", &display[display.len().saturating_sub(39)..])
    } else {
        display
    };

    Span::raw(display)
}

fn render_context(app: &AppState) -> Span<'_> {
    // TODO: track context window usage from model events
    Span::styled("—/—", Style::default().fg(app.theme.dim))
}

fn render_cost(app: &AppState) -> Span<'_> {
    // TODO: accumulate cost from usage events
    Span::styled("—", Style::default().fg(app.theme.dim))
}
