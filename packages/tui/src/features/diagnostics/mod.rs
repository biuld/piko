//! Read-only diagnostic overlay: turn diff.
//!
//! Fed by the existing host wire command (`TurnDiffGet`) and optional push
//! `TurnDiff` events. Presentation-only.

use piko_protocol::TurnDiffEvent;
use piko_tui_layout::{Component, SurfacePanel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::HitId;
use crate::navigation::SurfaceId;
use crate::theme::Theme;
use crate::ui::components::pane::{PaneFooter, PaneSpec, render_pane};
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

use super::centered_rect;

pub struct DiagnosticsCtx<'a> {
    pub theme: &'a Theme,
    pub hints: Option<&'a str>,
}

impl Component<HitId, DiagnosticsCtx<'_>> for DiagnosticsPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &DiagnosticsCtx<'_>) {
        self.render(frame, area, ctx.theme, ctx.hints);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let popup = centered_rect(82, 70, area);
        let spec = PaneSpec::new(self.title.as_str())
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true);
        spec.content_rect(popup)
            .map(|rect| vec![(rect, HitId::Content)])
            .unwrap_or_default()
    }
}

impl PointerComponent<HitId> for DiagnosticsPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        if hit.element == Some(HitId::Content) {
            match gesture {
                PointerGesture::ScrollUp => self.scroll_up(3),
                PointerGesture::ScrollDown => self.scroll_down(3),
                PointerGesture::Activate => {}
            }
        }
        Vec::new()
    }
}

impl SurfacePanel<SurfaceId, HitId, DiagnosticsCtx<'_>> for DiagnosticsPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Diagnostics
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticsKind {
    #[default]
    Diff,
}

/// Scrollable diagnostic text panel.
#[derive(Default)]
pub struct DiagnosticsPanel {
    kind: DiagnosticsKind,
    title: String,
    lines: Vec<String>,
    scroll: u16,
}

impl DiagnosticsPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll_up(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: u16) {
        let max = self.lines.len().saturating_sub(1) as u16;
        self.scroll = (self.scroll.saturating_add(n)).min(max);
    }

    pub fn set_diff(&mut self, diff: &TurnDiffEvent) {
        self.kind = DiagnosticsKind::Diff;
        self.title = format!("turn diff · {}", short(&diff.turn_id));
        self.scroll = 0;
        self.lines = format_diff(diff);
    }

    pub fn set_message(
        &mut self,
        kind: DiagnosticsKind,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.kind = kind;
        self.title = title.into();
        self.scroll = 0;
        self.lines = vec![message.into()];
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme, hints: Option<&str>) {
        let popup = centered_rect(82, 70, area);
        let title = self.title.as_str();
        let mut spec = PaneSpec::new(title).focused(true);
        if let Some(hints) = hints.filter(|value| !value.is_empty()) {
            spec = spec.hints(hints);
        } else {
            spec = spec.footer(PaneFooter::Reserved { height: 1 });
        }
        let Some(areas) = render_pane(frame, popup, &spec, theme) else {
            return;
        };

        let styled: Vec<Line<'_>> = self
            .lines
            .iter()
            .map(|line| style_diagnostic_line(line, self.kind, theme))
            .collect();

        let paragraph = Paragraph::new(styled)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        frame.render_widget(paragraph, areas.content);
    }
}

fn short(id: &str) -> String {
    id.chars().take(10).collect()
}

fn format_diff(diff: &TurnDiffEvent) -> Vec<String> {
    let mut lines = vec![
        format!("session  {}", diff.session_id),
        format!("turn     {}", diff.turn_id),
        format!("files    {}", diff.files.len()),
        String::new(),
    ];
    if diff.files.is_empty() && diff.unified_diff.trim().is_empty() {
        lines.push("No file changes recorded for this turn.".into());
        return lines;
    }
    for file in &diff.files {
        lines.push(format!("· {}", file.path));
    }
    if !diff.files.is_empty() {
        lines.push(String::new());
    }
    if diff.unified_diff.trim().is_empty() {
        lines.push("(no unified diff text)".into());
    } else {
        for line in diff.unified_diff.lines() {
            lines.push(line.to_string());
        }
    }
    lines
}

fn style_diagnostic_line<'a>(line: &'a str, kind: DiagnosticsKind, theme: &Theme) -> Line<'a> {
    let style = match kind {
        DiagnosticsKind::Diff if line.starts_with('+') && !line.starts_with("+++") => {
            Style::default().fg(theme.diff_insert_fg)
        }
        DiagnosticsKind::Diff if line.starts_with('-') && !line.starts_with("---") => {
            Style::default().fg(theme.diff_delete_fg)
        }
        DiagnosticsKind::Diff if line.starts_with("@@") => Style::default().fg(theme.info),
        _ if line.starts_with("──") => Style::default().fg(theme.muted),
        _ => Style::default().fg(theme.text),
    };
    Line::from(Span::styled(line, style))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diff_empty() {
        let diff = TurnDiffEvent {
            session_id: "s".into(),
            turn_id: "t".into(),
            files: vec![],
            unified_diff: String::new(),
        };
        let lines = format_diff(&diff);
        assert!(lines.iter().any(|l| l.contains("No file changes")));
    }

    #[test]
    fn format_diff_unified() {
        let diff = TurnDiffEvent {
            session_id: "s".into(),
            turn_id: "t".into(),
            files: vec![piko_protocol::TurnFileChange {
                path: "a.rs".into(),
                before: None,
                after: None,
            }],
            unified_diff: "--- a\n+++ b\n@@\n-old\n+new\n".into(),
        };
        let lines = format_diff(&diff);
        assert!(lines.iter().any(|l| l.contains("a.rs")));
        assert!(lines.iter().any(|l| l == "+new"));
    }

    #[test]
    fn wheel_scrolls_only_diagnostic_content() {
        let mut panel = DiagnosticsPanel::new();
        panel.lines = (0..10).map(|i| i.to_string()).collect();
        let hit = ComponentHit {
            element: Some(HitId::Content),
            rect: Rect::new(0, 0, 10, 5),
            x: 1,
            y: 1,
        };
        panel.pointer_event(hit, PointerGesture::ScrollDown);
        assert_eq!(panel.scroll, 3);
        panel.pointer_event(
            ComponentHit {
                element: None,
                ..hit
            },
            PointerGesture::ScrollDown,
        );
        assert_eq!(panel.scroll, 3);
    }
}
