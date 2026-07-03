use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::{ConnectorKind, SummaryPromptState, TreeFilterMode, TreePanel, visible};
use crate::theme::Theme;

impl TreePanel {
    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        summary_prompt: Option<&SummaryPromptState>,
        theme: &Theme,
    ) {
        frame.render_widget(Clear, area);

        let mut left_title = "Session Tree".to_string();
        if self.show_label_timestamps {
            left_title.push_str(" [+time]");
        }

        let mode_indicator = match self.filter_mode {
            TreeFilterMode::Default => "[Default] | NoTools | User | Labeled | All",
            TreeFilterMode::NoTools => "Default | [NoTools] | User | Labeled | All",
            TreeFilterMode::UserOnly => "Default | NoTools | [User] | Labeled | All",
            TreeFilterMode::LabeledOnly => "Default | NoTools | User | [Labeled] | All",
            TreeFilterMode::All => "Default | NoTools | User | Labeled | [All]",
        };

        let right_title = if self.visible.rows.is_empty() {
            format!("{}  0/0", mode_indicator)
        } else {
            format!(
                "{}  {}/{}",
                mode_indicator,
                self.selected_idx
                    .saturating_add(1)
                    .min(self.visible.rows.len()),
                self.visible.rows.len()
            )
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border))
            .title(Line::from(left_title).alignment(Alignment::Left))
            .title(Line::from(right_title).alignment(Alignment::Right));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 7 {
            return;
        }

        let help_text =
            "Ctrl+D/T/U/L/A filters · Tab/Shift+Tab cycle · Shift+L label · Alt+←/→ fold";
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                help_text,
                Style::default().fg(theme.muted),
            ))),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        if let Some(editor) = &self.label_editor {
            let label_line = Line::from(vec![
                Span::styled(
                    "Label (Enter to save, Esc to cancel): ",
                    Style::default().fg(theme.accent),
                ),
                Span::styled(&editor.input, Style::default().fg(theme.text)),
                Span::raw("█"),
            ]);
            frame.render_widget(
                Paragraph::new(label_line),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            );
        } else {
            let search_line = Line::from(vec![
                Span::raw("Search: "),
                Span::styled(filter, Style::default().fg(theme.accent)),
            ]);
            frame.render_widget(
                Paragraph::new(search_line),
                Rect::new(inner.x, inner.y + 2, inner.width, 1),
            );
        }

        let prompt_height = summary_prompt
            .map(|state| if state.input_active { 7 } else { 6 })
            .unwrap_or(0)
            .min(inner.height.saturating_sub(5));
        let footer_height = if summary_prompt.is_some() {
            prompt_height
        } else {
            1
        };
        let list_y = inner.y + 4;
        let list_h = inner.height.saturating_sub(5 + footer_height);
        let list_area = Rect::new(inner.x, list_y, inner.width, list_h);

        if self.visible.rows.is_empty() {
            let msg = if filter.is_empty() {
                "No entries found."
            } else {
                "No entries match the filter."
            };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(theme.muted)),
                list_area,
            );
        } else {
            let items: Vec<ListItem<'_>> = self
                .visible
                .rows
                .iter()
                .enumerate()
                .map(|(idx, row)| self.render_row(row, idx == self.selected_idx, theme))
                .collect();

            let mut state = ListState::default();
            state.select(Some(self.selected_idx.min(items.len().saturating_sub(1))));
            let list = List::new(items);
            frame.render_stateful_widget(list, list_area, &mut state);
        }

        if let Some(state) = summary_prompt
            && prompt_height > 0
        {
            let prompt_area = Rect::new(
                inner.x,
                inner.y + inner.height - prompt_height,
                inner.width,
                prompt_height,
            );
            crate::features::tree::summary_prompt::render_summary_prompt(
                frame,
                prompt_area,
                state,
                theme,
            );
        } else {
            let footer_text = "Enter confirm · Esc close";
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    footer_text,
                    Style::default().fg(theme.muted),
                ))),
                Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1),
            );
        }
    }

    fn render_row(
        &self,
        row: &visible::TreeRow,
        is_selected: bool,
        theme: &Theme,
    ) -> ListItem<'static> {
        let bg = if is_selected {
            theme.get("selectedBg")
        } else {
            Color::Reset
        };
        let styled = |text: String, color| {
            let style = Style::default().fg(color).bg(bg);
            let style = if is_selected {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            };
            Span::styled(text, style)
        };

        let mut spans = Vec::new();
        spans.push(styled(
            if is_selected { "› " } else { "  " }.to_string(),
            theme.accent,
        ));

        let prefix = tree_row_prefix(row);
        if !prefix.is_empty() {
            spans.push(styled(prefix, theme.dim));
        }

        if row.is_folded {
            spans.push(styled("⊞ ".to_string(), theme.accent));
        }
        if row.is_active_path {
            spans.push(styled("• ".to_string(), theme.accent));
        }

        if let Some(label) = &row.label {
            let label_text = if self.show_label_timestamps {
                format!(
                    "[{} - {}] ",
                    label.text.as_deref().unwrap_or(""),
                    label.timestamp
                )
            } else {
                format!("[{}] ", label.text.as_deref().unwrap_or(""))
            };
            spans.push(styled(label_text, theme.warning));
        }

        spans.push(styled(
            row.role_preview.clone(),
            self.role_color(row, theme),
        ));
        spans.push(styled(" ".to_string(), theme.text));
        spans.push(styled(row.text_preview.clone(), theme.text));

        ListItem::new(Line::from(spans))
    }

    fn role_color(&self, row: &visible::TreeRow, theme: &Theme) -> Color {
        if row.role_preview.contains("assistant") {
            theme.success
        } else if row.role_preview.contains("user") {
            theme.accent
        } else if row.role_preview.contains("branch") || row.role_preview.contains("compact") {
            theme.warning
        } else {
            theme.dim
        }
    }
}

pub(crate) fn tree_row_prefix(row: &visible::TreeRow) -> String {
    let connector_position = match row.connector {
        ConnectorKind::Branch | ConnectorKind::Corner => row.depth.checked_sub(1),
        ConnectorKind::Vertical | ConnectorKind::None => None,
    };
    let mut prefix = String::new();
    for level in 0..row.depth {
        if Some(level) == connector_position {
            match row.connector {
                ConnectorKind::Branch => prefix.push_str("├─ "),
                ConnectorKind::Corner => prefix.push_str("└─ "),
                _ => prefix.push_str("   "),
            }
        } else if row
            .gutters
            .iter()
            .any(|g| g.position == level && g.kind == ConnectorKind::Vertical)
        {
            prefix.push_str("│  ");
        } else {
            prefix.push_str("   ");
        }
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::tree_row_prefix;
    use crate::features::tree::visible::{ConnectorKind, Gutter, TreeRow};

    #[test]
    fn tree_row_prefix_places_connector_at_depth_position() {
        let row = TreeRow {
            entry_id: "branch".into(),
            depth: 2,
            connector: ConnectorKind::Corner,
            gutters: Vec::new(),
            is_active_path: false,
            is_folded: false,
            label: None,
            text_preview: String::new(),
            role_preview: String::new(),
        };

        assert_eq!(tree_row_prefix(&row), "   └─ ");
    }

    #[test]
    fn tree_row_prefix_preserves_vertical_gutters() {
        let row = TreeRow {
            entry_id: "descendant".into(),
            depth: 3,
            connector: ConnectorKind::Branch,
            gutters: vec![Gutter {
                position: 0,
                kind: ConnectorKind::Vertical,
            }],
            is_active_path: false,
            is_folded: false,
            label: None,
            text_preview: String::new(),
            role_preview: String::new(),
        };

        assert_eq!(tree_row_prefix(&row), "│     ├─ ");
    }
}
