use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};

use crate::app::ToolStatus;

use super::{preview_text, short_id};

/// A single entry displayed in the timeline.
#[derive(Clone)]
pub enum TimelineEntry {
    System(String),
    User(String),
    Assistant(String),
    Tool(ToolEntry),
    Session(String),
    Error(String),
}

/// Tool call state tracked inside the timeline.
#[derive(Clone)]
pub struct ToolEntry {
    pub id: String,
    pub name: String,
    pub status: ToolStatus,
    pub args: String,
    pub result: Option<String>,
    pub parent_message_id: Option<String>,
}

impl ToolEntry {
    pub fn is_error(&self) -> bool {
        self.status == ToolStatus::Failed
    }
}

/// In-memory ring buffer of timeline entries plus scroll state.
pub struct Timeline {
    pub entries: VecDeque<TimelineEntry>,
    pub scroll: usize,
    pub pending_new_items: usize,
    pub stream_text: String,
    pub tools_expanded: bool,
    /// Running and completed tool calls — kept for lookup/update.
    pub tool_calls: Vec<ToolEntry>,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            scroll: 0,
            pending_new_items: 0,
            stream_text: String::new(),
            tools_expanded: false,
            tool_calls: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: TimelineEntry) {
        let is_at_bottom = self.scroll == 0;
        self.entries.push_back(entry);
        if is_at_bottom {
            self.scroll = 0;
        } else {
            self.scroll = self.scroll.saturating_add(1);
            self.pending_new_items = self.pending_new_items.saturating_add(1);
        }
        while self.entries.len() > 500 {
            self.entries.pop_front();
            self.scroll = self.scroll.saturating_sub(1);
        }
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
        if self.scroll == 0 {
            self.pending_new_items = 0;
        }
    }

    pub fn jump_latest(&mut self) {
        self.scroll = 0;
        self.pending_new_items = 0;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.tool_calls.clear();
        self.scroll = 0;
        self.pending_new_items = 0;
        self.stream_text.clear();
    }

    /// Update or insert a tool in the registry. Returns `true` if an existing
    /// timeline entry was found and updated in-place.
    pub fn upsert_tool(&mut self, tool: ToolEntry) -> bool {
        // update registry
        if let Some(existing) = self.tool_calls.iter_mut().find(|t| t.id == tool.id) {
            *existing = tool.clone();
        } else {
            self.tool_calls.push(tool.clone());
        }
        // update timeline in-place if present
        for entry in self.entries.iter_mut().rev() {
            if let TimelineEntry::Tool(existing) = entry
                && existing.id == tool.id
            {
                *existing = tool;
                return true;
            }
        }
        false
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let mut items: Vec<ListItem<'_>> = self
            .entries
            .iter()
            .map(|entry| timeline_item(entry, self.tools_expanded))
            .collect();

        if !self.stream_text.is_empty() {
            items.push(ListItem::new(vec![
                Line::from(Span::styled(
                    "assistant",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(self.stream_text.as_str()),
            ]));
        }
        if items.is_empty() {
            items.push(ListItem::new(Line::from(Span::styled(
                "Type a prompt and press Enter.",
                Style::default().fg(Color::DarkGray),
            ))));
        }

        let max_items = usize::from(area.height.saturating_sub(2))
            .saturating_div(2)
            .max(1);
        let total = items.len();
        let end = total.saturating_sub(self.scroll.min(total));
        let start = end.saturating_sub(max_items);
        let items = items
            .into_iter()
            .skip(start)
            .take(end - start)
            .collect::<Vec<_>>();

        let title = if self.pending_new_items > 0 {
            format!("timeline | {} new items", self.pending_new_items)
        } else {
            "timeline".to_string()
        };
        let widget = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(widget, area);
    }
}

// ── private rendering helpers ────────────────────────────────────────────────

fn timeline_item(entry: &TimelineEntry, tools_expanded: bool) -> ListItem<'_> {
    match entry {
        TimelineEntry::System(text) => labeled_item("system", Color::Cyan, text),
        TimelineEntry::User(text) => labeled_item("user", Color::Yellow, text),
        TimelineEntry::Assistant(text) => labeled_item("assistant", Color::Green, text),
        TimelineEntry::Tool(tool) => tool_item(tool, tools_expanded),
        TimelineEntry::Session(text) => labeled_item("session", Color::Blue, text),
        TimelineEntry::Error(text) => labeled_item("error", Color::Red, text),
    }
}

fn labeled_item<'a>(label: &'a str, color: Color, text: &'a str) -> ListItem<'a> {
    ListItem::new(vec![
        Line::from(Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(text),
    ])
}

fn tool_item(tool: &ToolEntry, tools_expanded: bool) -> ListItem<'_> {
    let color = if tool.is_error() {
        Color::Red
    } else if tool.status == ToolStatus::Running {
        Color::Yellow
    } else {
        Color::Magenta
    };
    let status = match tool.status {
        ToolStatus::Running => "running",
        ToolStatus::Completed => "completed",
        ToolStatus::Failed => "failed",
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("tool {} [{status}] {}", tool.name, short_id(&tool.id)),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))];
    if tools_expanded {
        if let Some(parent) = &tool.parent_message_id {
            lines.push(Line::from(Span::styled(
                format!("parent message {}", short_id(parent)),
                Style::default().fg(Color::DarkGray),
            )));
        }
        if !tool.args.is_empty() {
            lines.push(Line::from(Span::styled(
                "args",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(tool.args.as_str()));
        }
        if let Some(result) = &tool.result {
            lines.push(Line::from(Span::styled(
                "result",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(result.as_str()));
        }
    } else if let Some(result) = &tool.result {
        lines.push(Line::from(Span::styled(
            preview_text(result),
            Style::default().fg(Color::DarkGray),
        )));
    } else if !tool.args.is_empty() {
        lines.push(Line::from(Span::styled(
            preview_text(&tool.args),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "details folded; use Ctrl-K -> Toggle tool details",
            Style::default().fg(Color::DarkGray),
        )));
    }
    ListItem::new(lines)
}
