use piko_protocol::agents::AgentSpec;
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row},
};

use crate::theme::Theme;
use crate::ui::components::filterable_list::FilterableList;
use crate::ui::components::table_panel::{ActionPrompt, TableBody, TablePanel};

pub struct AgentList {
    pub list: FilterableList<AgentSpec>,
    pub filter: String,
    pub loading: bool,
}

const WIDTHS: [Constraint; 4] = [
    Constraint::Length(12),
    Constraint::Length(15),
    Constraint::Length(12),
    Constraint::Min(20),
];

impl AgentList {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
            filter: String::new(),
            loading: true,
        }
    }

    pub fn load(&mut self, agents: Vec<AgentSpec>) {
        // Sort agents by id for stable display
        let mut sorted = agents;
        sorted.sort_by(|a, b| a.id.cmp(&b.id));

        self.list = FilterableList::new(sorted);
        self.loading = false;
        self.list.selected = 0;
    }

    pub fn move_up(&mut self) {
        let filter_clone = self.filter.clone();
        self.list
            .select_prev(&filter_clone, |item| Self::matches(item, &filter_clone));
    }

    pub fn move_down(&mut self) {
        let filter_clone = self.filter.clone();
        self.list
            .select_next(&filter_clone, |item| Self::matches(item, &filter_clone));
    }

    fn matches(item: &AgentSpec, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_lowercase();
        item.id.to_lowercase().contains(&q)
            || item.name.to_lowercase().contains(&q)
            || item.role.to_lowercase().contains(&q)
            || item
                .description
                .as_ref()
                .map(|d| d.to_lowercase().contains(&q))
                .unwrap_or(false)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let indices = self
            .list
            .filtered_indices(&self.filter, |item| Self::matches(item, &self.filter));
        let selected_filtered_idx = if indices.is_empty() {
            0
        } else {
            indices
                .iter()
                .position(|&idx| idx == self.list.selected)
                .unwrap_or(0)
        };

        let counter = if indices.is_empty() {
            "0".to_string()
        } else {
            format!("{}/{}", selected_filtered_idx + 1, indices.len())
        };

        let search_line = if self.filter.is_empty() {
            vec![Span::styled(
                "filter agents...",
                Style::default().fg(theme.muted),
            )]
        } else {
            vec![
                Span::styled("filter: ", Style::default().fg(theme.muted)),
                Span::raw(&self.filter),
            ]
        };

        let body = if self.loading {
            TableBody::Message(Paragraph::new(Span::styled(
                "Loading agents...",
                Style::default().fg(theme.muted),
            )))
        } else if indices.is_empty() {
            let msg = if self.filter.is_empty() {
                "No agents configured."
            } else {
                "No agents match filter."
            };
            TableBody::Message(Paragraph::new(Span::styled(
                msg,
                Style::default().fg(theme.muted),
            )))
        } else {
            let rows: Vec<Row> = indices
                .iter()
                .enumerate()
                .map(|(i, &idx)| {
                    let item = &self.list.items[idx];
                    let is_selected = i == selected_filtered_idx;

                    let (id_style, name_style) = if is_selected {
                        (
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        (
                            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                            Style::default().fg(theme.text),
                        )
                    };

                    let cells = vec![
                        Cell::from(Line::from(vec![Span::styled(&item.id, id_style)])),
                        Cell::from(Line::from(vec![Span::styled(&item.name, name_style)])),
                        Cell::from(Line::from(vec![Span::styled(
                            &item.role,
                            Style::default().fg(theme.accent),
                        )])),
                        Cell::from(Line::from(vec![Span::styled(
                            item.description.as_deref().unwrap_or(""),
                            Style::default().fg(theme.muted),
                        )])),
                    ];

                    Row::new(cells)
                })
                .collect();

            TableBody::Rows {
                widths: &WIDTHS,
                rows,
                selected_idx: selected_filtered_idx,
            }
        };

        let panel = TablePanel {
            left_title: "Agents".to_string(),
            mode_indicator: "read-only".to_string(),
            counter,
            help_text: "Edit .piko/agents/*.toml to configure",
            search_line: search_line.into(),
            body,
            action_prompt: ActionPrompt::Legend("↑/↓ select · Esc close"),
            gap: false,
        };

        panel.render(frame, area, theme);
    }
}
