use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SummaryChoice {
    NoSummary,
    DefaultSummary,
    CustomInstructions,
}

pub struct SummaryPromptState {
    pub target_entry_id: String,
    pub choices: Vec<SummaryChoice>,
    pub selected_idx: usize,
    pub custom_input: String,
    pub input_active: bool,
}

impl SummaryPromptState {
    pub fn new(target_entry_id: String) -> Self {
        Self {
            target_entry_id,
            choices: vec![
                SummaryChoice::NoSummary,
                SummaryChoice::DefaultSummary,
                SummaryChoice::CustomInstructions,
            ],
            selected_idx: 0,
            custom_input: String::new(),
            input_active: false,
        }
    }

    pub fn select_next(&mut self) {
        if !self.input_active && self.selected_idx + 1 < self.choices.len() {
            self.selected_idx += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if !self.input_active && self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    pub fn push_char(&mut self, ch: char) {
        if self.input_active {
            self.custom_input.push(ch);
        }
    }

    pub fn pop_char(&mut self) {
        if self.input_active {
            self.custom_input.pop();
        }
    }
}

pub fn render_summary_prompt(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &SummaryPromptState,
    theme: &Theme,
) {
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(theme.border_accent));
    frame.render_widget(block, area);

    let inner = prompt_content_area(area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Branch switch: ", Style::default().fg(theme.warning)),
            Span::styled(
                "leave current-path entries behind?",
                Style::default().fg(theme.text),
            ),
        ]),
        Line::default(),
        Line::from(choice_spans(state, theme)),
    ];

    if state.input_active {
        lines.push(Line::from(vec![
            Span::styled("Custom: ", Style::default().fg(theme.accent)),
            Span::styled(&state.custom_input, Style::default().fg(theme.text)),
            Span::raw("█"),
        ]));
    }

    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "Enter confirm · Esc cancel · Left/Right choose",
        Style::default().fg(theme.muted),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn choice_spans<'a>(state: &SummaryPromptState, theme: &Theme) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    for (i, choice) in state.choices.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("    "));
        }
        let text = match choice {
            SummaryChoice::NoSummary => "No summary",
            SummaryChoice::DefaultSummary => "Summarize",
            SummaryChoice::CustomInstructions => "Custom prompt",
        };
        let style = if i == state.selected_idx {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        let marker = if i == state.selected_idx {
            "› "
        } else {
            "  "
        };
        spans.push(Span::styled(marker, style));
        spans.push(Span::styled(text.to_string(), style));
    }
    spans
}

fn prompt_content_area(area: Rect) -> Rect {
    let horizontal_padding = if area.width > 8 { 3 } else { 1 };
    Rect::new(
        area.x + horizontal_padding,
        area.y.saturating_add(1),
        area.width.saturating_sub(horizontal_padding * 2),
        area.height.saturating_sub(1),
    )
}
