use piko_protocol::{SessionListScope, SessionSummary};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};

use crate::theme::Theme;
use crate::ui::components::filterable_list::FilterableList;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionScope {
    CurrentFolder,
    All,
}

impl SessionScope {
    pub fn to_protocol(self) -> SessionListScope {
        match self {
            SessionScope::CurrentFolder => SessionListScope::CurrentFolder,
            SessionScope::All => SessionListScope::All,
        }
    }
}

/// Resume Session panel.
pub struct SessionList {
    pub list: FilterableList<SessionSummary>,
    pub scope: SessionScope,
    pub named_only: bool,
    pub show_path: bool,
    pub loading: bool,
    pub error: Option<String>,
}

impl SessionList {
    pub fn new() -> Self {
        Self {
            list: FilterableList::new(Vec::new()),
            scope: SessionScope::CurrentFolder,
            named_only: false,
            show_path: false,
            loading: false,
            error: None,
        }
    }

    pub fn load(&mut self, mut sessions: Vec<SessionSummary>) {
        // Sort sessions by modified_at descending, then created_at, then session_id.
        sessions.sort_by(|a, b| {
            let a_mod = a.modified_at.as_deref().unwrap_or("");
            let b_mod = b.modified_at.as_deref().unwrap_or("");
            let cmp = b_mod.cmp(a_mod); // descending
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
            let a_cre = a.created_at.as_deref().unwrap_or("");
            let b_cre = b.created_at.as_deref().unwrap_or("");
            let cmp2 = b_cre.cmp(a_cre); // descending
            if cmp2 != std::cmp::Ordering::Equal {
                return cmp2;
            }
            a.session_id.cmp(&b.session_id)
        });
        self.list = FilterableList::new(sessions);
        self.loading = false;
        self.error = None;
    }

    pub fn select_next(&mut self, filter: &str) {
        let named_only = self.named_only;
        let show_path = self.show_path;
        self.list.select_next(filter, |item| {
            if named_only && item.name.is_none() {
                return false;
            }
            Self::matches_item_static(item, filter, show_path)
        });
    }

    pub fn select_prev(&mut self, filter: &str) {
        let named_only = self.named_only;
        let show_path = self.show_path;
        self.list.select_prev(filter, |item| {
            if named_only && item.name.is_none() {
                return false;
            }
            Self::matches_item_static(item, filter, show_path)
        });
    }

    pub fn selected_session_id(&self, filter: &str) -> Option<String> {
        self.selected_session_summary(filter).map(|s| s.session_id)
    }

    pub fn selected_session_summary(&self, filter: &str) -> Option<SessionSummary> {
        let filtered = self
            .list
            .filtered_indices(filter, |item| self.filter_matches(item, filter));
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
            .cloned()
    }

    pub fn filter_matches(&self, item: &SessionSummary, filter: &str) -> bool {
        if self.named_only && item.name.is_none() {
            return false;
        }
        Self::matches_item_static(item, filter, self.show_path)
    }

    fn matches_item_static(item: &SessionSummary, filter: &str, show_path: bool) -> bool {
        let f = filter.to_lowercase();
        if item.session_id.to_lowercase().contains(&f) {
            return true;
        }
        if item.cwd.to_lowercase().contains(&f) {
            return true;
        }
        if item
            .name
            .as_ref()
            .is_some_and(|n| n.to_lowercase().contains(&f))
        {
            return true;
        }
        if item
            .first_message
            .as_ref()
            .is_some_and(|msg| msg.to_lowercase().contains(&f))
        {
            return true;
        }
        if show_path
            && item
                .session_path
                .as_ref()
                .is_some_and(|path| path.to_lowercase().contains(&f))
        {
            return true;
        }
        false
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        active_session_id: Option<&str>,
        theme: &Theme,
    ) {
        frame.render_widget(Clear, area);

        let scope_title = match self.scope {
            SessionScope::CurrentFolder => "Resume Session (Current Folder)",
            SessionScope::All => "Resume Session (All)",
        };

        let scope_indicator = match self.scope {
            SessionScope::CurrentFolder => "[Current] | All",
            SessionScope::All => "Current | [All]",
        };
        let right_title = format!("{}  Recent", scope_indicator);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(Line::from(scope_title).alignment(Alignment::Left))
            .title(Line::from(right_title).alignment(Alignment::Right));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 6 {
            return;
        }

        // Sub-header line: Help
        let help_text = "Tab scope · Ctrl+N named · path";
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                help_text,
                Style::default().fg(theme.muted),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // Search line
        let search_line = Line::from(vec![
            Span::raw("Search: "),
            Span::styled(filter, Style::default().fg(theme.accent)),
        ]);
        frame.render_widget(
            Paragraph::new(search_line),
            Rect::new(inner.x, inner.y + 2, inner.width, 1),
        );

        // List / body area
        let list_y = inner.y + 4;
        let list_h = inner.height.saturating_sub(5);
        let list_area = Rect::new(inner.x, list_y, inner.width, list_h);

        if self.loading {
            frame.render_widget(
                Paragraph::new("Loading sessions...").style(Style::default().fg(theme.muted)),
                list_area,
            );
        } else if let Some(err) = &self.error {
            frame.render_widget(
                Paragraph::new(format!("Error: {}", err)).style(Style::default().fg(theme.error)),
                list_area,
            );
        } else {
            let filtered: Vec<(usize, &SessionSummary)> = self
                .list
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| self.filter_matches(item, filter))
                .collect();

            if filtered.is_empty() {
                let msg = if filter.is_empty() {
                    if self.scope == SessionScope::CurrentFolder {
                        "No sessions in this folder. Press Tab to view All sessions."
                    } else {
                        "No sessions found."
                    }
                } else {
                    "No sessions match the filter."
                };
                frame.render_widget(
                    Paragraph::new(msg).style(Style::default().fg(theme.muted)),
                    list_area,
                );
            } else {
                let mut widths = vec![
                    Constraint::Fill(1), // Title
                ];
                if self.show_path || self.scope == SessionScope::All {
                    widths.push(Constraint::Percentage(30)); // Cwd/Path
                }
                widths.extend([
                    Constraint::Length(12), // Message count
                    Constraint::Length(8),  // Age
                    Constraint::Length(8),  // Active
                ]);

                let rows: Vec<Row> = filtered
                    .iter()
                    .map(|&(orig_idx, item)| {
                        let is_selected = orig_idx == self.list.selected;
                        let marker = if is_selected { " › " } else { "   " };
                        let marker_style = if is_selected {
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };

                        let title_text = if let Some(n) = &item.name {
                            n.clone()
                        } else if let Some(msg) = &item.first_message {
                            let cleaned: String = msg
                                .chars()
                                .map(|c| {
                                    if c == '\n' || c == '\r' || c == '\t' {
                                        ' '
                                    } else {
                                        c
                                    }
                                })
                                .collect();
                            let char_count = cleaned.chars().count();
                            if char_count > 40 {
                                let truncated: String = cleaned.chars().take(37).collect();
                                format!("{}...", truncated)
                            } else {
                                cleaned
                            }
                        } else {
                            "untitled".to_string()
                        };

                        let title_style = if is_selected {
                            Style::default()
                                .fg(theme.accent)
                                .add_modifier(Modifier::BOLD)
                        } else if item.name.is_some() {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(theme.text)
                        };

                        let mut cells = vec![Cell::from(Line::from(vec![
                            Span::styled(marker, marker_style),
                            Span::styled(title_text, title_style),
                        ]))];

                        if self.show_path || self.scope == SessionScope::All {
                            let path_str = if self.show_path && item.session_path.is_some() {
                                item.session_path.clone().unwrap()
                            } else {
                                item.cwd.clone()
                            };
                            cells.push(Cell::from(Line::from(Span::styled(
                                path_str,
                                Style::default().fg(theme.muted),
                            ))));
                        }

                        let count = item.message_count;
                        let count_str = if count == 1 {
                            "1 message".to_string()
                        } else {
                            format!("{} messages", count)
                        };
                        cells.push(Cell::from(
                            Line::from(Span::styled(count_str, Style::default().fg(theme.muted)))
                                .alignment(Alignment::Right),
                        ));

                        let age_str = format_age(item.modified_at.as_deref());
                        cells.push(Cell::from(
                            Line::from(Span::styled(age_str, Style::default().fg(theme.muted)))
                                .alignment(Alignment::Right),
                        ));

                        let is_active = active_session_id
                            .map(|id| id == item.session_id)
                            .unwrap_or(false);
                        let active_span = if is_active {
                            Span::styled(
                                "active",
                                Style::default()
                                    .fg(theme.accent)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::raw("")
                        };
                        cells.push(Cell::from(
                            Line::from(active_span).alignment(Alignment::Right),
                        ));

                        Row::new(cells)
                    })
                    .collect();

                let selected_filtered_idx = filtered
                    .iter()
                    .position(|&(orig_idx, _)| orig_idx == self.list.selected)
                    .unwrap_or(0)
                    .min(filtered.len().saturating_sub(1));

                let mut table_state =
                    TableState::default().with_selected(Some(selected_filtered_idx));
                let table_widget = Table::new(rows, widths).row_highlight_style(Style::default());

                frame.render_stateful_widget(table_widget, list_area, &mut table_state);
            }
        }

        // Footer line
        let footer_text = "Enter resume · Esc close";
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                footer_text,
                Style::default().fg(theme.muted),
            ))),
            Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
        );
    }
}

fn format_age(timestamp_str: Option<&str>) -> String {
    let Some(t_str) = timestamp_str else {
        return "".to_string();
    };
    let Ok(ms) = t_str.parse::<u64>() else {
        return "".to_string();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let diff_secs = now_ms.saturating_sub(ms) / 1000;
    if diff_secs < 60 {
        "just now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h", diff_secs / 3600)
    } else {
        format!("{}d", diff_secs / 86400)
    }
}
