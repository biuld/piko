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
    /// Approximate tokens in the current context (last model prompt size).
    pub context_used: Option<u64>,
    /// Active model's context window size.
    pub context_total: Option<u64>,
    /// Session cumulative cost in USD when usage has been projected.
    pub cost_usd: Option<f64>,
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
                    BottomBarItem::Context => {
                        render_context(view.context_used, view.context_total, view.theme)
                    }
                    BottomBarItem::Cost => render_cost(view.cost_usd, view.theme),
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

fn render_context(used: Option<u64>, total: Option<u64>, theme: &Theme) -> Span<'static> {
    let text = format_context(used, total);
    let style = if used.is_none() && total.is_none() {
        Style::default().fg(theme.dim)
    } else {
        Style::default()
    };
    Span::styled(text, style)
}

fn render_cost(cost_usd: Option<f64>, theme: &Theme) -> Span<'static> {
    match cost_usd {
        None => Span::styled("—", Style::default().fg(theme.dim)),
        Some(cost) => Span::raw(format_cost(cost)),
    }
}

/// Human-readable context fill: `12.2k/200k`, partial `12.2k/—`, or `—/—`.
pub fn format_context(used: Option<u64>, total: Option<u64>) -> String {
    match (used, total) {
        (None, None) => "—/—".to_string(),
        (Some(used), None) => format!("{}/—", format_tokens(used)),
        (None, Some(total)) => format!("—/{}", format_tokens(total)),
        (Some(used), Some(total)) => {
            format!("{}/{}", format_tokens(used), format_tokens(total))
        }
    }
}

/// Session cost in USD (`$0.42`, `$0.0042` for small amounts).
pub fn format_cost(cost_usd: f64) -> String {
    if !cost_usd.is_finite() || cost_usd < 0.0 {
        return "—".to_string();
    }
    if cost_usd == 0.0 {
        return "$0.00".to_string();
    }
    if cost_usd >= 0.01 {
        format!("${cost_usd:.2}")
    } else {
        format!("${cost_usd:.4}")
    }
}

/// Compact token counts for the status row (`1.5k`, `200k`, `1.2M`).
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        let m = n as f64 / 1_000_000.0;
        if n.is_multiple_of(1_000_000) {
            format!("{}M", n / 1_000_000)
        } else {
            format!("{m:.1}M")
        }
    } else if n >= 1000 {
        if n.is_multiple_of(1000) {
            format!("{}k", n / 1000)
        } else {
            format!("{:.1}k", n as f64 / 1000.0)
        }
    } else {
        n.to_string()
    }
}

/// Prompt-side tokens for an approximate context-window fill.
pub fn context_tokens_from_usage(usage: &piko_protocol::messages::Usage) -> u64 {
    usage.input.saturating_add(usage.cache_read)
}

#[cfg(test)]
mod tests {
    use super::{format_context, format_cost, format_tokens};

    #[test]
    fn format_tokens_humanizes() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1k");
        assert_eq!(format_tokens(12_200), "12.2k");
        assert_eq!(format_tokens(200_000), "200k");
        assert_eq!(format_tokens(1_000_000), "1M");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn format_context_placeholders() {
        assert_eq!(format_context(None, None), "—/—");
        assert_eq!(format_context(Some(12_200), None), "12.2k/—");
        assert_eq!(format_context(None, Some(200_000)), "—/200k");
        assert_eq!(format_context(Some(12_200), Some(200_000)), "12.2k/200k");
    }

    #[test]
    fn format_cost_scales() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(0.42), "$0.42");
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(f64::NAN), "—");
    }
}
