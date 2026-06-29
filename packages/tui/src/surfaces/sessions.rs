use piko_protocol::SessionSummary;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem},
};

use super::short_id;

/// Sessions list overlay.
pub struct SessionsOverlay {
    pub sessions: Vec<SessionSummary>,
    pub selected: usize,
}

impl SessionsOverlay {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            selected: 0,
        }
    }

    pub fn load(&mut self, sessions: Vec<SessionSummary>) {
        self.sessions = sessions;
        self.selected = self.selected.min(self.sessions.len().saturating_sub(1));
    }

    pub fn select_next(&mut self) {
        if !self.sessions.is_empty() {
            self.selected = (self.selected + 1).min(self.sessions.len() - 1);
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn selected_session_id(&self) -> Option<String> {
        self.sessions
            .get(self.selected)
            .map(|s| s.session_id.clone())
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);
        let items: Vec<ListItem<'_>> = if self.sessions.is_empty() {
            vec![ListItem::new(Line::from("No sessions returned by hostd."))]
        } else {
            self.sessions
                .iter()
                .enumerate()
                .map(|(idx, session)| session_item(idx == self.selected, session))
                .collect()
        };
        let widget = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("sessions | j/k select | Enter open | Esc close"),
        );
        frame.render_widget(widget, area);
    }
}

fn session_item(selected: bool, session: &SessionSummary) -> ListItem<'_> {
    let marker = if selected { "> " } else { "  " };
    let name = session.name.as_deref().unwrap_or("untitled");
    let id = short_id(&session.session_id);
    let style = if selected {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    ListItem::new(vec![
        Line::from(Span::styled(format!("{marker}{name}  {id}"), style)),
        Line::from(Span::styled(
            format!("  {}  seq {}", session.cwd, session.seq),
            Style::default().fg(Color::DarkGray),
        )),
    ])
}
