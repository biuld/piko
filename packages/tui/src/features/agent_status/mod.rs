//! AgentPanel — agent list in slot B between Timeline and Editor.
//!
//! Shows active agents with status indicators and tree connectors for
//! parent-child spawn relationships. Supports selection with ↑↓ and
//! Enter to switch the viewed agent.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{app::QueueStatus, theme::Theme};

/// Agent entry displayed in the panel.
#[derive(Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub task_id: String,
    pub name: String,
    pub parent_task_id: Option<String>,
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
    pub active_task_id: Option<String>,
    pub focus: bool,
}

pub struct AgentPanelView<'a> {
    pub state: &'a AgentPanelState,
    pub is_running: bool,
    pub queue: &'a QueueStatus,
    pub spinner_frame: usize,
    pub theme: &'a Theme,
}

impl AgentPanelState {
    pub fn render(frame: &mut Frame<'_>, area: Rect, view: AgentPanelView<'_>) {
        let agent_count = view.state.agents.len();
        let has_queue = view.queue.steer_count > 0
            || view.queue.follow_up_count > 0
            || view.queue.next_turn_count > 0;

        let mut lines = Vec::new();

        if agent_count == 0 {
            lines.push(render_idle_agent_row(view.theme.accent));
        } else {
            let prefixes = build_tree_prefixes(&view.state.agents);

            for (i, agent) in view.state.agents.iter().enumerate() {
                let is_selected = view.state.focus && i == view.state.selected_idx;
                let is_active = view.state.active_task_id.as_deref() == Some(&agent.task_id);

                let prefix = prefixes[i].as_str();
                lines.push(render_agent_row(
                    agent,
                    prefix,
                    is_selected,
                    is_active,
                    view.is_running,
                    view.spinner_frame,
                    view.theme,
                ));
            }

            if !view.is_running && has_queue {
                let total_queue = view.queue.steer_count
                    + view.queue.follow_up_count
                    + view.queue.next_turn_count;
                lines.push(Line::from(vec![Span::styled(
                    format!("  {} queued", total_queue),
                    Style::default().fg(view.theme.dim),
                )]));
            }
        }

        let border_color = if view.state.focus {
            view.theme.accent
        } else {
            view.theme.border_muted
        };

        let widget = Paragraph::new(lines).block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::TOP)
                .border_style(Style::default().fg(border_color)),
        );
        frame.render_widget(widget, area);
    }

    pub fn height(&self) -> u16 {
        let agent_count = self.agents.len();
        let base = if agent_count == 0 {
            1
        } else {
            agent_count as u16
        };
        base + 1 // +1 for top border
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
        if let Some(existing) = self.agents.iter_mut().find(|a| a.task_id == agent.task_id) {
            existing.agent_id = agent.agent_id;
            existing.name = agent.name;
            if agent.parent_task_id.is_some() {
                existing.parent_task_id = agent.parent_task_id;
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
        let Some(parent_id) = agent.parent_task_id.as_deref() else {
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
                .find(|a| a.task_id == id)
                .and_then(|a| a.parent_task_id.clone());
        }

        // Build indentation from outermost to innermost
        let mut s = String::new();
        for anc_id in ancestor_ids.iter().rev() {
            let continues = agents[i + 1..]
                .iter()
                .any(|a| a.parent_task_id.as_deref() == Some(anc_id));
            if continues {
                s.push_str("│ ");
            } else {
                s.push_str("  ");
            }
        }

        // Connector for this agent
        let is_last = !agents[i + 1..]
            .iter()
            .any(|a| a.parent_task_id.as_deref() == Some(parent_id));
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
    is_running: bool,
    frame_idx: usize,
    theme: &Theme,
) -> Line<'static> {
    let status_char = if matches!(agent.activity, piko_protocol::AgentActivity::Running { .. })
        && (is_running || agent.parent_task_id.is_some())
    {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        frames[frame_idx % frames.len()]
    } else {
        match agent.status {
            piko_protocol::AgentStatus::Running => "●",
            piko_protocol::AgentStatus::Completed => "✓",
            piko_protocol::AgentStatus::Failed => "✗",
            piko_protocol::AgentStatus::Cancelled => "✗",
            piko_protocol::AgentStatus::Closed => "×",
            _ => "●",
        }
    };

    let status_color = match agent.status {
        piko_protocol::AgentStatus::Running => theme.warning,
        piko_protocol::AgentStatus::Completed => theme.success,
        piko_protocol::AgentStatus::Failed
        | piko_protocol::AgentStatus::Cancelled
        | piko_protocol::AgentStatus::Closed => theme.error,
        _ => theme.accent,
    };

    let mut name_style = Style::default();
    if is_active {
        name_style = name_style.add_modifier(Modifier::BOLD).fg(theme.accent);
    }
    if is_selected {
        name_style = name_style.add_modifier(Modifier::REVERSED);
    }

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

    Line::from(vec![
        Span::raw(indent.to_string()),
        Span::styled(status_char, Style::default().fg(status_color)),
        Span::raw(" "),
        Span::styled(agent.name.clone(), name_style),
        Span::styled(lifecycle, Style::default().fg(theme.dim)),
        Span::styled(unread, Style::default().fg(theme.warning)),
    ])
}

fn render_idle_agent_row(accent: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled("●", Style::default().fg(accent)),
        Span::raw(" "),
        Span::styled("main", Style::default().add_modifier(Modifier::BOLD)),
    ])
}
