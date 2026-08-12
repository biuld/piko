use piko_protocol::{AgentUsageSummary, Usage};
use piko_tui_layout::{Component, SurfacePanel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::HitId,
    features::bottom_bar::{format_cost, format_tokens},
    navigation::SurfaceId,
    theme::Theme,
    ui::{
        components::pane::{PaneSpec, render_pane},
        line_layout::truncate_cols,
    },
};

/// Read-only, host-authoritative per-AgentInstance resource accounting.
pub struct UsagePanel;

#[derive(Clone, Copy)]
pub struct UsageCtx<'a> {
    pub rows: &'a [AgentUsageSummary],
    pub scroll: usize,
    pub session_usage: Option<&'a Usage>,
    pub viewed_agent_instance_id: Option<&'a str>,
    pub has_session: bool,
    pub theme: &'a Theme,
}

impl Component<HitId, UsageCtx<'_>> for UsagePanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &UsageCtx<'_>) {
        let spec = PaneSpec::new("usage")
            .hints("↑/↓ scroll · Esc close")
            .focused(true);
        let Some(areas) = render_pane(frame, area, &spec, ctx.theme) else {
            return;
        };
        let lines = if !ctx.has_session {
            vec![Line::from(Span::styled(
                "No active session.",
                Style::default().fg(ctx.theme.dim),
            ))]
        } else if ctx.rows.is_empty() {
            vec![Line::from(Span::styled(
                "No agent usage recorded yet.",
                Style::default().fg(ctx.theme.dim),
            ))]
        } else if areas.content.width >= 94 {
            wide_lines(ctx, areas.content.height)
        } else {
            compact_lines(ctx, areas.content.height)
        };
        frame.render_widget(Paragraph::new(lines), areas.content);
    }

    fn component_regions(&self, _area: Rect) -> Vec<(Rect, HitId)> {
        Vec::new()
    }
}

impl SurfacePanel<SurfaceId, HitId, UsageCtx<'_>> for UsagePanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Usage
    }
}

fn wide_lines<'a>(ctx: &UsageCtx<'a>, height: u16) -> Vec<Line<'a>> {
    let visible = usize::from(height.saturating_sub(3)).max(1);
    let (start, end) = visible_range(ctx.rows.len(), ctx.scroll, visible);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "{:<22} {:>5} {:>8} {:>8} {:>8} {:>8} {:>10}  {}",
            format!("agent ({}-{}/{})", start + 1, end, ctx.rows.len()),
            "runs",
            "input",
            "output",
            "cache",
            "total",
            "active",
            "cost"
        ),
        Style::default().fg(ctx.theme.dim),
    ))];
    for row in &ctx.rows[start..end] {
        let marker = if ctx.viewed_agent_instance_id == Some(row.agent_instance_id.as_str()) {
            "›"
        } else {
            " "
        };
        let agent = format!("{marker} {}", truncate_cols(&agent_label(row), 20));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{agent:<22}"),
                if marker == "›" {
                    Style::default().fg(ctx.theme.accent)
                } else {
                    Style::default()
                },
            ),
            Span::raw(format!(
                " {:>5} {:>8} {:>8} {:>8} {:>8} {:>10}  {}",
                optional_count(row.run_count),
                format_tokens(row.usage.input),
                format_tokens(row.usage.output),
                format_tokens(row.usage.cache_read),
                format_tokens(row.usage.total_tokens),
                optional_duration(row.active_duration_ms),
                cost_text(&row.usage),
            )),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(session_line(ctx.session_usage, ctx.theme));
    lines
}

fn compact_lines<'a>(ctx: &UsageCtx<'a>, height: u16) -> Vec<Line<'a>> {
    let visible = usize::from(height.saturating_sub(3) / 2).max(1);
    let (start, end) = visible_range(ctx.rows.len(), ctx.scroll, visible);
    let mut lines = vec![Line::from(Span::styled(
        format!("agents {}–{} of {}", start + 1, end, ctx.rows.len()),
        Style::default().fg(ctx.theme.dim),
    ))];
    for row in &ctx.rows[start..end] {
        let marker = if ctx.viewed_agent_instance_id == Some(row.agent_instance_id.as_str()) {
            "›"
        } else {
            " "
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} {}", agent_label(row)),
                if marker == "›" {
                    Style::default().fg(ctx.theme.accent)
                } else {
                    Style::default()
                },
            ),
            Span::styled(
                format!(
                    " · {} runs · {} · {}",
                    optional_count(row.run_count),
                    optional_duration(row.active_duration_ms),
                    cost_text(&row.usage)
                ),
                Style::default().fg(ctx.theme.dim),
            ),
        ]));
        lines.push(Line::from(format!(
            "  in {} · out {} · cache {} · total {}",
            format_tokens(row.usage.input),
            format_tokens(row.usage.output),
            format_tokens(row.usage.cache_read),
            format_tokens(row.usage.total_tokens),
        )));
    }
    lines.push(Line::from(""));
    lines.push(session_line(ctx.session_usage, ctx.theme));
    lines
}

fn visible_range(len: usize, scroll: usize, visible: usize) -> (usize, usize) {
    let start = scroll.min(len.saturating_sub(1));
    (start, start.saturating_add(visible).min(len))
}

fn session_line<'a>(usage: Option<&Usage>, theme: &'a Theme) -> Line<'a> {
    let Some(usage) = usage else {
        return Line::from(Span::styled(
            "session total  —",
            Style::default().fg(theme.dim),
        ));
    };
    Line::from(vec![
        Span::styled("session total", Style::default().fg(theme.accent)),
        Span::raw(format!(
            "  in {} · out {} · cache {} · total {} · {}",
            format_tokens(usage.input),
            format_tokens(usage.output),
            format_tokens(usage.cache_read),
            format_tokens(usage.total_tokens),
            cost_text(usage),
        )),
    ])
}

fn agent_label(row: &AgentUsageSummary) -> String {
    let short_id = row.agent_instance_id.chars().take(8).collect::<String>();
    if row.agent_id == row.agent_instance_id {
        short_id
    } else {
        format!("{} · {short_id}", row.agent_id)
    }
}

fn cost_text(usage: &Usage) -> String {
    if usage.cost.entries.is_empty() {
        "—".to_string()
    } else {
        format_cost(&usage.cost)
    }
}

pub fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = total_seconds % 3600 / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds:02}s")
    } else {
        format!("{seconds}s")
    }
}

fn optional_count(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| value.to_string())
}

fn optional_duration(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_string(), format_duration)
}

#[cfg(test)]
mod tests {
    use super::{agent_label, format_duration};
    use piko_protocol::{AgentUsageSummary, Usage};

    #[test]
    fn duration_is_compact_and_stable() {
        assert_eq!(format_duration(999), "0s");
        assert_eq!(format_duration(65_000), "1m 05s");
        assert_eq!(format_duration(3_661_000), "1h 01m");
    }

    #[test]
    fn label_disambiguates_agent_instances() {
        let row = AgentUsageSummary {
            agent_instance_id: "agent-instance-123".into(),
            agent_id: "reviewer".into(),
            run_count: Some(1),
            active_duration_ms: Some(1),
            usage: Usage::empty(),
        };
        assert_eq!(agent_label(&row), "reviewer · agent-in");
    }
}
