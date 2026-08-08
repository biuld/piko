//! BottomBar — always-visible status row at the bottom of the TUI (shell chrome).
//!
//! Compact session projection: agent · model · cwd · context · cost.
//! Read-only. Full agent tree is a Browse surface (F4), not a plane strip.

use std::path::Path;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{config::bottom_bar::BottomBarItem, theme::Theme, ui::components::spinner_glyph};

pub struct BottomBar;

pub struct BottomBarView<'a> {
    pub items: &'a [BottomBarItem],
    /// Compact agent label, e.g. `main`, `main ⠋`, `·3`.
    pub agent: Option<&'a str>,
    pub agent_busy: bool,
    pub spinner_frame: usize,
    pub model_id: Option<&'a str>,
    pub thinking_level: Option<&'a str>,
    pub cwd: &'a Path,
    pub context_used: Option<u64>,
    pub context_total: Option<u64>,
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
                    BottomBarItem::Agent => {
                        render_agent(view.agent, view.agent_busy, view.spinner_frame, view.theme)
                    }
                    BottomBarItem::Model => render_model(view.model_id, view.thinking_level),
                    BottomBarItem::Cwd => render_cwd(view.cwd),
                    BottomBarItem::Context => {
                        render_context(view.context_used, view.context_total, view.theme)
                    }
                    BottomBarItem::Cost => render_cost(view.cost_usd, view.theme),
                };
                [
                    Span::raw(" "),
                    separator(view.theme.dim),
                    Span::raw(" "),
                    span,
                ]
            })
            .collect();

        let items = if items.len() >= 3 {
            &items[3..]
        } else {
            &items[..]
        };

        let line = Line::from(items.to_vec());
        let paragraph = Paragraph::new(line).style(Style::default().fg(view.theme.muted));
        frame.render_widget(paragraph, area);
    }
}

fn separator(dim: ratatui::style::Color) -> Span<'static> {
    Span::styled("·", Style::default().fg(dim))
}

fn render_agent(
    agent: Option<&str>,
    busy: bool,
    spinner_frame: usize,
    theme: &Theme,
) -> Span<'static> {
    match agent {
        None => Span::styled("—", Style::default().fg(theme.dim)),
        Some(name) if busy => {
            let spin = spinner_glyph(spinner_frame);
            Span::styled(format!("{name} {spin}"), Style::default().fg(theme.accent))
        }
        Some(name) => Span::raw(name.to_string()),
    }
}

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
        assert_eq!(format_tokens(1500), "1.5k");
    }

    #[test]
    fn format_context_pairs() {
        assert_eq!(format_context(None, None), "—/—");
        assert_eq!(format_context(Some(1200), Some(200_000)), "1.2k/200k");
    }

    #[test]
    fn format_cost_usd() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(0.42), "$0.42");
        assert_eq!(format_cost(0.0042), "$0.0042");
    }
}
