//! Agent instances panel — Browse surface (`SurfaceId::Agents`).
//!
//! Compact agent status also projects into BottomBar chrome; this module
//! paints the full selectable tree when the surface is open.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    app::QueueStatus,
    layout::{DEFAULT_HORIZONTAL_INSET, inset_horizontal},
    theme::Theme,
    ui::components::{
        ACTIVE_MARKER, FAIL_GLYPH, IDLE_MARKER, SUCCESS_GLYPH, selection_prefix, spinner_glyph,
    },
};

/// Agent entry displayed in the panel.
#[derive(Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub agent_instance_id: String,
    pub name: String,
    pub parent_agent_instance_id: Option<String>,
    pub lifecycle: piko_protocol::AgentInstanceLifecycle,
    pub activity: piko_protocol::AgentActivity,
    pub unread_report_count: u32,
    pub status: piko_protocol::AgentStatus,
}

/// AgentPanel state (maintained in AppState).
#[derive(Default)]
pub struct AgentPanelState {
    pub agents: Vec<AgentEntry>,
    pub selected_idx: usize,
    pub active_agent_instance_id: Option<String>,
    pub focus: bool,
    /// Set only after an authoritative agent projection (reconcile / AgentList).
    pub agents_hydrated: bool,
}

pub struct AgentPanelView<'a> {
    pub state: &'a AgentPanelState,
    /// Foreground projection for each agent_instance_id (parallel to `state.agents`).
    pub foreground: &'a [piko_protocol::AgentForeground],
    pub queue: &'a QueueStatus,
    pub spinner_frame: usize,
    pub theme: &'a Theme,
}

impl AgentPanelState {
    pub fn is_loading(&self) -> bool {
        !self.agents_hydrated
    }

    pub fn mark_hydrated(&mut self) {
        self.agents_hydrated = true;
    }

    pub fn begin_loading(&mut self) {
        self.agents.clear();
        self.active_agent_instance_id = None;
        self.selected_idx = 0;
        self.agents_hydrated = false;
    }

    pub fn render(frame: &mut Frame<'_>, area: Rect, view: AgentPanelView<'_>) {
        let agent_count = view.state.agents.len();
        let has_queue = view.queue.steer_count > 0
            || view.queue.follow_up_count > 0
            || view.queue.next_turn_count > 0;
        let any_busy = view.foreground.iter().any(|fg| fg.is_busy());

        let mut lines = Vec::new();

        if view.state.is_loading() {
            lines.push(crate::ui::components::feedback::loading_line(
                view.spinner_frame,
                view.theme,
            ));
        } else if agent_count == 0 {
            lines.push(render_empty_agent_row(view.theme.dim));
        } else {
            let prefixes = build_tree_prefixes(&view.state.agents);

            for (i, agent) in view.state.agents.iter().enumerate() {
                let is_selected = view.state.focus && i == view.state.selected_idx;
                let is_active = view.state.active_agent_instance_id.as_deref()
                    == Some(&agent.agent_instance_id);
                let foreground = view
                    .foreground
                    .get(i)
                    .copied()
                    .unwrap_or(piko_protocol::AgentForeground::Idle);

                let prefix = prefixes[i].as_str();
                lines.push(render_agent_row(
                    agent,
                    prefix,
                    is_selected,
                    is_active,
                    foreground,
                    view.spinner_frame,
                    view.theme,
                ));
            }

            if !any_busy && has_queue {
                let total_queue = view.queue.steer_count
                    + view.queue.follow_up_count
                    + view.queue.next_turn_count;
                lines.push(Line::from(vec![Span::styled(
                    format!("  {} queued", total_queue),
                    Style::default().fg(view.theme.dim),
                )]));
            }
        }

        // Focused strip uses panel border; passive uses muted (Selected ≠ Focused).
        let border_color = if view.state.focus {
            view.theme.border
        } else {
            view.theme.border_muted
        };

        // Top rule stays edge-flush; only agent rows get horizontal inset.
        let border = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color));
        let content_area = inset_horizontal(border.inner(area), DEFAULT_HORIZONTAL_INSET);
        frame.render_widget(border, area);
        frame.render_widget(Paragraph::new(lines), content_area);
    }

    pub fn select_next(&mut self) {
        if !self.agents.is_empty() {
            self.selected_idx = (self.selected_idx + 1).min(self.agents.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_idx = self.selected_idx.saturating_sub(1);
    }

    pub fn selected_agent(&self) -> Option<&AgentEntry> {
        self.agents.get(self.selected_idx)
    }

    pub fn upsert_agent(&mut self, agent: AgentEntry) {
        if let Some(existing) = self
            .agents
            .iter_mut()
            .find(|a| a.agent_instance_id == agent.agent_instance_id)
        {
            existing.agent_id = agent.agent_id;
            existing.name = agent.name;
            if agent.parent_agent_instance_id.is_some() {
                existing.parent_agent_instance_id = agent.parent_agent_instance_id;
            }
            existing.status = agent.status;
            existing.lifecycle = agent.lifecycle;
            existing.activity = agent.activity;
            existing.unread_report_count = agent.unread_report_count;
        } else {
            self.agents.push(agent);
        }
    }
}

// ── tree prefix builder ──────────────────────────────────────────────────────

/// Build tree connector prefix for each agent.
///
/// Root agents get no prefix (spinner at left margin).
/// Children get "├─ " or "└─ " with "│ " continuation lines for
/// ancestors that have more descendants coming.
fn build_tree_prefixes(agents: &[AgentEntry]) -> Vec<String> {
    let n = agents.len();
    let mut prefixes = Vec::with_capacity(n);

    for i in 0..n {
        let agent = &agents[i];
        let Some(parent_id) = agent.parent_agent_instance_id.as_deref() else {
            prefixes.push(String::new());
            continue;
        };

        // Collect ancestors from innermost to outermost
        let mut ancestor_ids: Vec<String> = Vec::new();
        let mut current = Some(parent_id.to_string());
        while let Some(id) = current.take() {
            ancestor_ids.push(id.clone());
            current = agents[..i]
                .iter()
                .find(|a| a.agent_instance_id == id)
                .and_then(|a| a.parent_agent_instance_id.clone());
        }

        // Build indentation from outermost to innermost
        let mut s = String::new();
        for anc_id in ancestor_ids.iter().rev() {
            let continues = agents[i + 1..]
                .iter()
                .any(|a| a.parent_agent_instance_id.as_deref() == Some(anc_id));
            if continues {
                s.push_str("│ ");
            } else {
                s.push_str("  ");
            }
        }

        // Connector for this agent
        let is_last = !agents[i + 1..]
            .iter()
            .any(|a| a.parent_agent_instance_id.as_deref() == Some(parent_id));
        if is_last {
            s.push_str("└─ ");
        } else {
            s.push_str("├─ ");
        }

        prefixes.push(s);
    }

    prefixes
}

// ── rendering ────────────────────────────────────────────────────────────────

fn render_agent_row(
    agent: &AgentEntry,
    indent: &str,
    is_selected: bool,
    is_active: bool,
    foreground: piko_protocol::AgentForeground,
    frame_idx: usize,
    theme: &Theme,
) -> Line<'static> {
    let (status_char, status_color) = match foreground {
        piko_protocol::AgentForeground::Running | piko_protocol::AgentForeground::Cancelling => {
            (spinner_glyph(frame_idx), theme.warning)
        }
        piko_protocol::AgentForeground::RequiresAction => (ACTIVE_MARKER, theme.warning),
        piko_protocol::AgentForeground::Queued => (IDLE_MARKER, theme.muted),
        piko_protocol::AgentForeground::Idle => match agent.status {
            piko_protocol::AgentStatus::Running => (ACTIVE_MARKER, theme.warning),
            piko_protocol::AgentStatus::Completed => (SUCCESS_GLYPH, theme.success),
            piko_protocol::AgentStatus::Failed | piko_protocol::AgentStatus::Cancelled => {
                (FAIL_GLYPH, theme.error)
            }
            piko_protocol::AgentStatus::Closed => (FAIL_GLYPH, theme.error),
            _ if is_active => (ACTIVE_MARKER, theme.accent),
            _ => (IDLE_MARKER, theme.dim),
        },
    };

    // Selected ≠ Active: selection uses ❯ + accent text; active uses status glyph.
    let name_style = if is_selected {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else if is_active {
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text)
    };

    let lifecycle = match agent.lifecycle {
        piko_protocol::AgentInstanceLifecycle::Open => String::new(),
        piko_protocol::AgentInstanceLifecycle::Closed => " closed".into(),
        piko_protocol::AgentInstanceLifecycle::Terminated => " terminated".into(),
        piko_protocol::AgentInstanceLifecycle::Unavailable => " unavailable".into(),
    };
    let unread = if agent.unread_report_count > 0 {
        format!(" +{}", agent.unread_report_count)
    } else {
        String::new()
    };

    let mut spans = vec![
        Span::styled(
            selection_prefix(is_selected),
            if is_selected {
                Style::default().fg(theme.accent)
            } else {
                Style::default()
            },
        ),
        Span::raw(indent.to_string()),
        Span::styled(status_char.to_string(), Style::default().fg(status_color)),
        Span::raw(" "),
        Span::styled(agent.name.clone(), name_style),
        Span::styled(lifecycle, Style::default().fg(theme.dim)),
        Span::styled(unread, Style::default().fg(theme.warning)),
    ];
    if is_active && !is_selected {
        // Quiet "current view" cue without stealing selection style.
        spans.push(Span::styled(" current", Style::default().fg(theme.muted)));
    }
    Line::from(spans)
}

fn render_empty_agent_row(dim: Color) -> Line<'static> {
    Line::from(vec![Span::styled("No agents", Style::default().fg(dim))])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::ui::components::feedback::loading_line;

    #[test]
    fn loading_until_hydrated_never_uses_fake_main_label() {
        let state = AgentPanelState::default();
        assert!(state.is_loading());

        let line = loading_line(0, &Theme::dark());
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("loading"));
        assert!(!text.contains("main"));
        assert!(!text.contains("Main"));
    }

    #[test]
    fn hydrated_empty_shows_explicit_empty_not_main() {
        let mut state = AgentPanelState::default();
        state.mark_hydrated();
        assert!(!state.is_loading());

        let line = render_empty_agent_row(Theme::dark().dim);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "No agents");
    }
}
