#![allow(dead_code)]

use crate::theme::Theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceOption {
    pub label: String,
    pub has_input: bool,
    pub input_prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub header: String,
    pub prompt: String,
    pub choices: Vec<ChoiceOption>,
    pub selected_idx: usize,
    pub input_value: String,
    pub is_input_active: bool,
}

impl Question {
    pub fn new(
        header: impl Into<String>,
        prompt: impl Into<String>,
        choices: Vec<ChoiceOption>,
    ) -> Self {
        Self {
            header: header.into(),
            prompt: prompt.into(),
            choices,
            selected_idx: 0,
            input_value: String::new(),
            is_input_active: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveWorkflow {
    pub questions: Vec<Question>,
    pub active_question_idx: usize,
    pub require_confirm: bool,
    pub confirm_focused: bool,
    pub is_completed: bool,
    pub target_entry_id: Option<String>,
}

impl InteractiveWorkflow {
    pub fn new(questions: Vec<Question>, require_confirm: bool) -> Self {
        Self {
            questions,
            active_question_idx: 0,
            require_confirm,
            confirm_focused: false,
            is_completed: false,
            target_entry_id: None,
        }
    }

    pub fn select_next(&mut self) {
        if self.questions.is_empty() {
            return;
        }
        let q = &mut self.questions[self.active_question_idx];
        if !q.is_input_active && q.selected_idx + 1 < q.choices.len() {
            q.selected_idx += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.questions.is_empty() {
            return;
        }
        let q = &mut self.questions[self.active_question_idx];
        if !q.is_input_active && q.selected_idx > 0 {
            q.selected_idx -= 1;
        }
    }

    pub fn select_choice(&mut self, idx: usize) {
        if self.questions.is_empty() {
            return;
        }
        let q = &mut self.questions[self.active_question_idx];
        if !q.is_input_active && idx < q.choices.len() {
            q.selected_idx = idx;
        }
    }

    pub fn next_step(&mut self) {
        if self.questions.is_empty() {
            return;
        }
        if self.active_question_idx + 1 < self.questions.len() {
            self.active_question_idx += 1;
            self.confirm_focused = false;
        } else if self.require_confirm {
            self.confirm_focused = true;
        }
    }

    pub fn prev_step(&mut self) {
        if self.confirm_focused {
            self.confirm_focused = false;
            return;
        }
        if self.active_question_idx > 0 {
            self.active_question_idx -= 1;
        }
    }

    pub fn can_submit(&self) -> bool {
        !self.questions.is_empty()
    }

    pub fn selected_answers(&self) -> Vec<(usize, usize, Option<String>)> {
        self.questions
            .iter()
            .enumerate()
            .filter_map(|(question_idx, question)| {
                question.choices.get(question.selected_idx).map(|choice| {
                    (
                        question_idx,
                        question.selected_idx,
                        choice.has_input.then(|| question.input_value.clone()),
                    )
                })
            })
            .collect()
    }

    pub fn input_active(&self) -> bool {
        if self.questions.is_empty() {
            return false;
        }
        self.questions[self.active_question_idx].is_input_active
    }

    pub fn set_input_active(&mut self, active: bool) {
        if self.questions.is_empty() {
            return;
        }
        self.questions[self.active_question_idx].is_input_active = active;
    }

    pub fn push_char(&mut self, ch: char) {
        if self.questions.is_empty() {
            return;
        }
        let q = &mut self.questions[self.active_question_idx];
        if q.is_input_active {
            q.input_value.push(ch);
        }
    }

    pub fn pop_char(&mut self) {
        if self.questions.is_empty() {
            return;
        }
        let q = &mut self.questions[self.active_question_idx];
        if q.is_input_active {
            q.input_value.pop();
        }
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(theme.border_accent));
        frame.render_widget(block, area);

        let inner = prompt_content_area(area);

        let mut lines = Vec::new();

        // Multi-question tab header
        if self.questions.len() > 1 {
            let mut tab_spans = Vec::new();
            for i in 0..self.questions.len() {
                if i > 0 {
                    tab_spans.push(Span::raw("   "));
                }
                let is_active = i == self.active_question_idx && !self.confirm_focused;
                let text = format!("[{}]", self.questions[i].header);
                let style = if is_active {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.muted)
                };
                tab_spans.push(Span::styled(text, style));
            }
            if self.require_confirm {
                tab_spans.push(Span::raw("   "));
                let text = "[Submit]".to_string();
                let style = if self.confirm_focused {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.muted)
                };
                tab_spans.push(Span::styled(text, style));
            }
            lines.push(Line::from(tab_spans));
            lines.push(Line::default());
        }

        if self.confirm_focused {
            lines.push(Line::from(Span::styled(
                "Ready to submit your answers?",
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::default());
            lines.push(Line::from(Span::styled(
                "❯ [ Confirm ]",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if !self.questions.is_empty() {
            let q = &self.questions[self.active_question_idx];
            // 1. Question Prompt
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", q.header),
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&q.prompt, Style::default().fg(theme.text)),
            ]));

            lines.push(Line::default());

            // 2. Render choices vertically
            for (i, choice) in q.choices.iter().enumerate() {
                let is_selected = i == q.selected_idx;
                let prefix = if is_selected { "❯ " } else { "  " };
                let num_str = format!("{}. ", i + 1);

                let style = if is_selected {
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.muted)
                };

                let mut spans = vec![
                    Span::styled(prefix, Style::default().fg(theme.accent)),
                    Span::styled(num_str, style),
                    Span::styled(choice.label.clone(), style),
                ];

                if is_selected && choice.has_input && q.is_input_active {
                    spans.push(Span::styled(": ", style));
                    spans.push(Span::styled(
                        &q.input_value,
                        Style::default().fg(theme.text),
                    ));
                    spans.push(Span::styled("█", Style::default().fg(theme.accent)));
                } else if !q.input_value.is_empty() && choice.has_input {
                    spans.push(Span::styled(
                        format!(": {}", q.input_value),
                        Style::default().fg(theme.muted),
                    ));
                }

                lines.push(Line::from(spans));
            }
        }

        lines.push(Line::default());

        // 3. Render help instructions
        let mut help_txt = "Enter to select · ↑/↓ to navigate · Esc to cancel";
        if self.confirm_focused {
            help_txt = "Enter to submit · Tab to cycle · Esc to cancel";
        } else if !self.questions.is_empty() {
            let q = &self.questions[self.active_question_idx];
            if q.is_input_active {
                help_txt = "Enter to save · Esc to exit editing";
            } else if self.questions.len() > 1 {
                help_txt = "Enter to select · ↑/↓ navigate · Tab switch question · Esc cancel";
            }
        }

        lines.push(Line::from(Span::styled(
            help_txt,
            Style::default().fg(theme.muted),
        )));

        frame.render_widget(Paragraph::new(lines), inner);
    }
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
