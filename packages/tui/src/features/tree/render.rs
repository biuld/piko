use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row},
};

use super::{ConnectorKind, TreeFilterMode, TreePanel, visible};
use crate::theme::Theme;
use crate::ui::components::interactive_workflow::InteractiveWorkflow;
use crate::ui::components::table_panel::{ActionPrompt, TableBody, TablePanel};

impl TreePanel {
    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        filter: &str,
        summary_prompt: Option<&InteractiveWorkflow>,
        theme: &Theme,
    ) {
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

        let counter = if self.visible.rows.is_empty() {
            "0/0".to_string()
        } else {
            format!(
                "{}/{}",
                self.selected_idx
                    .saturating_add(1)
                    .min(self.visible.rows.len()),
                self.visible.rows.len()
            )
        };

        let help_text = "Tab/Shift+Tab cycle · Shift+L label · Alt+←/→ fold";

        let search_line = if let Some(editor) = &self.label_editor {
            let mut spans = vec![Span::styled(
                "Label (Enter to save, Esc to cancel): ",
                Style::default().fg(theme.accent),
            )];
            let tb_line = editor.input.render_line(theme, true);
            spans.extend(tb_line.spans);
            Line::from(spans)
        } else {
            Line::from(vec![
                Span::raw("Search: "),
                Span::styled(filter, Style::default().fg(theme.accent)),
            ])
        };

        let body = if self.visible.rows.is_empty() {
            let msg = if filter.is_empty() {
                "No entries found."
            } else {
                "No entries match the filter."
            };
            TableBody::Message(Paragraph::new(msg).style(Style::default().fg(theme.muted)))
        } else {
            let rows: Vec<Row> = self
                .visible
                .rows
                .iter()
                .enumerate()
                .map(|(idx, row)| self.render_row(row, idx == self.selected_idx, theme))
                .collect();
            TableBody::Rows {
                widths: &[Constraint::Fill(1)],
                rows,
                selected_idx: self.selected_idx,
            }
        };

        // SummaryPrompt uses a constant height of 7 to prevent height updates from causing layout jitter
        let action_prompt = if let Some(state) = summary_prompt {
            ActionPrompt::Interactive {
                height: 7,
                render: Box::new(move |frame, area| {
                    state.render(frame, area, theme);
                }),
            }
        } else {
            ActionPrompt::Legend("Enter confirm · Esc close")
        };

        let panel = TablePanel {
            left_title,
            mode_indicator: mode_indicator.to_string(),
            counter,
            help_text,
            search_line,
            body,
            action_prompt,
            gap: true,
        };

        panel.render(frame, area, theme);
    }

    fn render_row(&self, row: &visible::TreeRow, is_selected: bool, theme: &Theme) -> Row<'static> {
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

        Row::new(vec![Cell::from(Line::from(spans))])
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
