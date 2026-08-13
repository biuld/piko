//! Read-only diagnostic overlays: turn diff, prompt debug.
//!
//! Fed by existing host wire commands (`TurnDiffGet`, `PromptDebugGet`) and
//! optional push `TurnDiff` events. Presentation-only.

use piko_protocol::{PromptDebugSnapshot, TurnDiffEvent};
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
use crate::ui::components::pane::{PaneSpec, render_pane};
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};

use super::centered_rect;

impl Component<HitId, Theme> for DiagnosticsPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let popup = centered_rect(82, 70, area);
        let spec = PaneSpec::new(self.title.as_str())
            .hints("↑/↓ scroll · Esc close")
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

impl SurfacePanel<SurfaceId, HitId, Theme> for DiagnosticsPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::Diagnostics
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticsKind {
    #[default]
    Diff,
    PromptDebug,
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

    pub fn set_prompt_debug(&mut self, snapshot: &PromptDebugSnapshot) {
        self.kind = DiagnosticsKind::PromptDebug;
        self.title = format!("prompt debug · {}", short(&snapshot.agent_instance_id));
        self.scroll = 0;
        self.lines = format_prompt_debug(snapshot);
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

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let popup = centered_rect(82, 70, area);
        let title = self.title.as_str();
        let spec = PaneSpec::new(title)
            .hints("↑/↓ scroll · Esc close")
            .focused(true);
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

fn format_prompt_debug(snapshot: &PromptDebugSnapshot) -> Vec<String> {
    let mut lines = vec![
        format!("session  {}", snapshot.session_id),
        format!("agent    {}", snapshot.agent_instance_id),
        format!("run      {}", snapshot.run_id),
        format!("tools    {}", snapshot.tool_catalog.tools.len()),
        format!("resources {} message(s)", snapshot.resource_messages.len()),
        format!("model inputs {}", snapshot.model_inputs.len()),
        String::new(),
        "── run prompt ──".into(),
    ];
    push_pretty_json(&mut lines, &snapshot.run_prompt, "run prompt");

    if !snapshot.resource_messages.is_empty() {
        lines.push(String::new());
        lines.push("── retained resources ──".into());
        push_pretty_json(&mut lines, &snapshot.resource_messages, "resources");
    }

    lines.push(String::new());
    lines.push("── tool catalog ──".into());
    push_pretty_json(&mut lines, &snapshot.tool_catalog, "tool catalog");

    if !snapshot.model_inputs.is_empty() {
        lines.push(String::new());
        lines.push("── model inputs ──".into());
        for (i, step) in snapshot.model_inputs.iter().enumerate() {
            lines.push(format!(
                "[{}] {}/{}  run={} step={}",
                i + 1,
                step.provider,
                step.model,
                short(&step.run_id),
                short(&step.step_id)
            ));
            lines.push("  request".into());
            push_pretty_json_indented(&mut lines, &step.request, "request", "    ");
            lines.push("  options".into());
            push_pretty_json_indented(&mut lines, &step.options, "options", "    ");
            lines.push(String::new());
        }
    }
    lines
}

fn push_pretty_json<T: serde::Serialize>(lines: &mut Vec<String>, value: &T, label: &str) {
    push_pretty_json_indented(lines, value, label, "");
}

fn push_pretty_json_indented<T: serde::Serialize>(
    lines: &mut Vec<String>,
    value: &T,
    label: &str,
    indent: &str,
) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => lines.extend(text.lines().map(|line| format!("{indent}{line}"))),
        Err(_) => lines.push(format!("{indent}(failed to format {label})")),
    }
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
    fn prompt_debug_renders_complete_sections_without_truncation() {
        let long_content = (0..100)
            .map(|index| format!("line-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let snapshot = PromptDebugSnapshot {
            session_id: "s".into(),
            agent_instance_id: "a".into(),
            run_id: "run-exact".into(),
            run_prompt: piko_protocol::SemanticRunPrompt {
                blocks: vec![piko_protocol::PromptBlock {
                    id: "project.context.0".into(),
                    kind: piko_protocol::PromptBlockKind::Instruction,
                    authority: piko_protocol::InstructionAuthority::Project,
                    trust: piko_protocol::ContentTrust::WorkspaceControlled,
                    source: piko_protocol::PromptSource::new("workspace-file", "AGENTS.md"),
                    content: long_content,
                    content_digest: "digest".into(),
                    cache_scope: piko_protocol::CacheScope::ResourceSnapshot,
                }],
                ..Default::default()
            },
            resource_messages: Vec::new(),
            tool_catalog: piko_protocol::ResolvedToolCatalog::new(Vec::new(), "tools"),
            model_inputs: vec![piko_protocol::ModelInputDebugSnapshot {
                session_id: "s".into(),
                agent_instance_id: "a".into(),
                run_id: "run-exact".into(),
                step_id: "step-1".into(),
                provider: "provider".into(),
                model: "model".into(),
                request: serde_json::json!({"input": "actual"}),
                options: serde_json::json!({"reasoning": "high"}),
            }],
        };

        let rendered = format_prompt_debug(&snapshot).join("\n");
        assert!(rendered.contains("run      run-exact"));
        assert!(rendered.contains("line-99"));
        assert!(rendered.contains("── tool catalog ──"));
        assert!(rendered.contains("\"reasoning\": \"high\""));
        assert!(!rendered.contains("truncated"));
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
