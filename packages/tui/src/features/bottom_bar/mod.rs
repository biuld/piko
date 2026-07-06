//! BottomBar — always-visible status row at the bottom of the TUI.
//!
//! Displays contextual session information as items separated by `·`.
//! No key hints or interactive prompts.  Configurable via `tui.bottomBar.*` settings.
//!
//! Default items: model · cwd · context · cost

use std::path::Path;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{config::bottom_bar::BottomBarItem, theme::Theme};

pub struct BottomBar;

pub struct BottomBarView<'a> {
    pub items: &'a [BottomBarItem],
    pub model_id: Option<&'a str>,
    pub thinking_level: Option<&'a str>,
    pub cwd: &'a Path,
    pub theme: &'a Theme,
}

impl BottomBar {
    pub fn render(frame: &mut Frame<'_>, area: Rect, view: BottomBarView<'_>) {
        let items: Vec<Span<'_>> = view
            .items
            .iter()
            .flat_map(|item| {
                let span = match item {
                    BottomBarItem::Model => render_model(view.model_id, view.thinking_level),
                    BottomBarItem::Cwd => render_cwd(view.cwd),
                    BottomBarItem::Context => render_context(view.theme),
                    BottomBarItem::Cost => render_cost(view.theme),
                };
                // Insert separator between items
                [
                    Span::raw(" "),
                    separator(view.theme.dim),
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
        let paragraph = Paragraph::new(line).style(Style::default().fg(view.theme.muted));
        frame.render_widget(paragraph, area);
    }
}

// ── separator ────────────────────────────────────────────────────────────────

fn separator(dim: ratatui::style::Color) -> Span<'static> {
    Span::styled("·", Style::default().fg(dim))
}

// ── item renderers ───────────────────────────────────────────────────────────

fn render_model<'a>(model_id: Option<&'a str>, thinking_level: Option<&'a str>) -> Span<'a> {
    let model = model_id.unwrap_or("—");
    let thinking = thinking_level.unwrap_or("off");

    let text = if thinking == "off" {
        model.to_string()
    } else {
        format!("{model} {thinking}")
    };

    Span::raw(text)
}

fn render_cwd(cwd: &Path) -> Span<'_> {
    let cwd_str = cwd.to_string_lossy();

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

fn render_context(theme: &Theme) -> Span<'_> {
    // TODO: track context window usage from model events
    Span::styled("—/—", Style::default().fg(theme.dim))
}

fn render_cost(theme: &Theme) -> Span<'_> {
    // TODO: accumulate cost from usage events
    Span::styled("—", Style::default().fg(theme.dim))
}
