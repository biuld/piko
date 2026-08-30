//! Read-only diagnostic overlay: Agent work diff.
//!
//! Fed by the existing host wire command (`AgentWorkDiffGet`) and optional push
//! `AgentWorkDiff` events. Presentation-only.

use piko_protocol::AgentWorkDiffEvent;
use piko_tui_layout::{Component, SurfacePanel, ViewportMetrics, ViewportState};
use ratatui::{Frame, layout::Rect, style::Style};
use std::cell::Cell;

use crate::app::HitId;
use crate::navigation::SurfaceId;
use crate::theme::Theme;
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};
use crate::ui::{
    components::{
        pane::{PaneFooter, PaneSpec, render_pane},
        scroll_view::paint_scroll_view,
    },
    text_layout::{Breakability, TextRun, wrap_runs},
};

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
            .map(|rect| {
                // The viewport reserves the scrollbar gutter before text
                // layout, including while content fits.  Do not let the
                // static content gate turn that gutter into a child hit.
                let content = Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), rect.height);
                (content.width > 0 && content.height > 0)
                    .then_some((content, HitId::Content))
                    .into_iter()
                    .collect()
            })
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
    viewport: Cell<ViewportState>,
}

impl DiagnosticsPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scroll_up(&mut self, n: u16) {
        let mut viewport = self.viewport.get();
        viewport.scroll_by(-(n as isize));
        self.viewport.set(viewport);
    }

    pub fn scroll_down(&mut self, n: u16) {
        let mut viewport = self.viewport.get();
        viewport.scroll_by(n as isize);
        self.viewport.set(viewport);
    }

    pub fn set_diff(&mut self, diff: &AgentWorkDiffEvent) {
        self.kind = DiagnosticsKind::Diff;
        self.title = format!("work diff · {}", short(&diff.root_input_id));
        self.viewport.get_mut().scroll_to(0);
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
        self.viewport.get_mut().scroll_to(0);
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

        let text_width = areas.content.width.saturating_sub(1).max(1);
        let mut runs = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            runs.push(TextRun::new(
                line.clone(),
                diagnostic_style(line, self.kind, theme),
                Breakability::Grapheme,
            ));
            if index + 1 < self.lines.len() {
                runs.push(TextRun::new(
                    String::new(),
                    Style::default(),
                    Breakability::HardBreak,
                ));
            }
        }
        let layout = wrap_runs(runs, usize::from(text_width));
        let mut viewport = self.viewport.get();
        viewport.update_metrics(ViewportMetrics::new(
            layout.row_count(),
            usize::from(areas.content.height),
        ));
        let plan = viewport.prepare(areas.content, 1);
        self.viewport.set(viewport);
        paint_scroll_view(frame, &layout, &plan, theme);
    }
}

fn short(id: &str) -> String {
    id.chars().take(10).collect()
}

fn format_diff(diff: &AgentWorkDiffEvent) -> Vec<String> {
    let mut lines = vec![
        format!("session  {}", diff.session_id),
        format!("input    {}", diff.root_input_id),
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

fn diagnostic_style(line: &str, kind: DiagnosticsKind, theme: &Theme) -> Style {
    match kind {
        DiagnosticsKind::Diff if line.starts_with('+') && !line.starts_with("+++") => {
            Style::default().fg(theme.diff_insert_fg)
        }
        DiagnosticsKind::Diff if line.starts_with('-') && !line.starts_with("---") => {
            Style::default().fg(theme.diff_delete_fg)
        }
        DiagnosticsKind::Diff if line.starts_with("@@") => Style::default().fg(theme.info),
        _ if line.starts_with("──") => Style::default().fg(theme.muted),
        _ => Style::default().fg(theme.text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diff_empty() {
        let diff = AgentWorkDiffEvent {
            session_id: "s".into(),
            root_input_id: "input-t".into(),
            files: vec![],
            unified_diff: String::new(),
        };
        let lines = format_diff(&diff);
        assert!(lines.iter().any(|l| l.contains("No file changes")));
    }

    #[test]
    fn format_diff_unified() {
        let diff = AgentWorkDiffEvent {
            session_id: "s".into(),
            root_input_id: "input-t".into(),
            files: vec![piko_protocol::AgentWorkFileChange {
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
        panel
            .viewport
            .get_mut()
            .update_metrics(ViewportMetrics::new(10, 5));
        let hit = ComponentHit {
            element: Some(HitId::Content),
            rect: Rect::new(0, 0, 10, 5),
            x: 1,
            y: 1,
        };
        panel.pointer_event(hit, PointerGesture::ScrollDown);
        assert_eq!(panel.viewport.get().top(), 3);
        panel.pointer_event(
            ComponentHit {
                element: None,
                ..hit
            },
            PointerGesture::ScrollDown,
        );
        assert_eq!(panel.viewport.get().top(), 3);
    }
}
