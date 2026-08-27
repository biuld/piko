//! ChoiceWorkflow — Decide-surface choice shell (Approval, Ask User, SummaryPrompt).
//!
//! Chrome always goes through [`Pane`](super::pane): features build a
//! [`PaneSpec`] and call [`ChoiceWorkflow::render_in_pane`], or paint the body
//! into a parent content / reserved-footer rect via [`ChoiceWorkflow::paint_body`].
//! This component owns questions, tabs, confirm, inline input, and choice
//! selection — not outer borders or title affixes.

use super::text_box::TextBox;
use crate::app::HitId;
use crate::theme::Theme;
use crate::ui::components::{
    feedback::row_primary_style,
    pane::{PaneSpec, PaneTitleAffix, render_pane},
    selection_prefix,
};
use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

mod layout;
mod pointer;

pub(crate) use layout::{clamp_rect, row_rect};

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
    pub input_value: TextBox,
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
            input_value: TextBox::new(),
            is_input_active: false,
        }
    }
}

/// Shared choice workflow: multi-question tabs, optional inline text, confirm
/// step. Used by Approval, Tool Interaction, and Tree SummaryPrompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceWorkflow {
    pub questions: Vec<Question>,
    pub active_question_idx: usize,
    pub require_confirm: bool,
    pub confirm_focused: bool,
    pub target_entry_id: Option<String>,
}

impl ChoiceWorkflow {
    pub fn new(questions: Vec<Question>, require_confirm: bool) -> Self {
        Self {
            questions,
            active_question_idx: 0,
            require_confirm,
            confirm_focused: false,
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

    /// Jump to a step. `step == questions.len()` is the Submit step when
    /// confirmation is required.
    pub fn goto_step(&mut self, step: usize) {
        if self.questions.is_empty() {
            return;
        }
        if step >= self.questions.len() {
            self.active_question_idx = self.questions.len().saturating_sub(1);
            self.confirm_focused = self.require_confirm;
        } else {
            self.active_question_idx = step;
            self.confirm_focused = false;
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
                        choice
                            .has_input
                            .then(|| question.input_value.text().to_string()),
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

    /// Content rows for dock height budgeting (body only; chrome is Pane).
    pub(crate) fn dock_content_rows(&self, theme: &Theme) -> u16 {
        self.body_lines(theme).len() as u16
    }

    #[cfg(test)]
    pub fn help_text(&self) -> String {
        if self.confirm_focused {
            "Enter to submit · Tab to cycle · Esc to cancel".into()
        } else if !self.questions.is_empty() {
            let q = &self.questions[self.active_question_idx];
            if q.is_input_active {
                "Enter to save · Esc to exit editing".into()
            } else if self.questions.len() > 1 {
                "Enter to select · ↑/↓ choose · Tab switch question · Esc cancel".into()
            } else {
                "↑↓ select · Enter confirm · Esc cancel".into()
            }
        } else {
            "Esc to cancel".into()
        }
    }

    /// Content rect for a standalone Decide dock. Guidance is projected by
    /// the resident row above the ComposerBand.
    pub fn modal_content_area(&self, area: Rect) -> Rect {
        PaneSpec::new("").content_rect(area).unwrap_or(area)
    }

    /// Body lines (tabs, prompt, choices, confirm) — shared paint source.
    fn body_lines<'a>(&'a self, theme: &Theme) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

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
                format!("{}[ Confirm ]", selection_prefix(true)),
                row_primary_style(true, theme).fg(theme.accent),
            )));
        } else if !self.questions.is_empty() {
            let q = &self.questions[self.active_question_idx];
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

            for (i, choice) in q.choices.iter().enumerate() {
                let is_selected = i == q.selected_idx;
                let prefix = selection_prefix(is_selected);
                let num_str = format!("{}. ", i + 1);
                let style = if is_selected {
                    row_primary_style(true, theme)
                } else {
                    Style::default().fg(theme.muted)
                };
                let caret_style = if is_selected {
                    Style::default().fg(theme.accent)
                } else {
                    Style::default()
                };

                let mut spans = vec![
                    Span::styled(prefix, caret_style),
                    Span::styled(num_str, style),
                    Span::styled(choice.label.clone(), style),
                ];

                if is_selected && choice.has_input && q.is_input_active {
                    spans.push(Span::styled(": ", style));
                    let tb_line = q.input_value.render_line(theme, true);
                    spans.extend(tb_line.spans);
                } else if !q.input_value.is_empty() && choice.has_input {
                    spans.push(Span::styled(
                        format!(": {}", q.input_value.text()),
                        Style::default().fg(theme.muted),
                    ));
                }

                lines.push(Line::from(spans));
            }
        }

        lines
    }

    /// Paint workflow body into a content rect (no chrome). Parent must be Pane.
    pub fn paint_body(
        &self,
        frame: &mut Frame<'_>,
        content: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        let lines = self.body_lines(theme);
        frame.render_widget(Paragraph::new(lines), content);
        self.paint_selected_and_hover(frame, content, theme, interaction);
    }

    /// Standalone Decide dock: Pane chrome + body.
    pub fn render_in_pane(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        title: &str,
        affixes: Vec<PaneTitleAffix>,
        interaction: InteractionState<HitId>,
    ) {
        // No elevated fill: Decide docks sit in the composer band like Models /
        // Agents. Pane still Clear's the host so the stream does not bleed
        // through; body bg stays terminal/base (not a second "card" layer).
        let spec = PaneSpec::new(title).title_affixes(affixes).focused(true);
        if let Some(areas) = render_pane(frame, area, &spec, theme) {
            self.paint_body(frame, areas.content, theme, interaction);
        }
    }

    /// Embedded body into a parent Pane reserved footer / content slot.
    /// No private Block — chrome belongs to the outer Pane.
    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        self.paint_body(frame, area, theme, InteractionState::default());
    }
}
