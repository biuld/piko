use super::text_box::TextBox;
use crate::app::HitId;
use crate::theme::Theme;
use crate::ui::components::{frame_border_style, hint_style, selection_prefix};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
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

/// Shared low-level workflow panel: choice-based prompts with optional inline
/// text input and an explicit Submit step. Used by approval prompts, tool
/// interaction workflows, and the tree summary prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractiveWorkflow {
    pub questions: Vec<Question>,
    pub active_question_idx: usize,
    pub require_confirm: bool,
    pub confirm_focused: bool,
    pub target_entry_id: Option<String>,
    /// When set, replaces the state-derived help line (e.g. approval's
    /// shortcut legend, which the generic choice help does not describe).
    pub help_override: Option<String>,
}

impl InteractiveWorkflow {
    pub fn new(questions: Vec<Question>, require_confirm: bool) -> Self {
        Self {
            questions,
            active_question_idx: 0,
            require_confirm,
            confirm_focused: false,
            target_entry_id: None,
            help_override: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help_override = Some(help.into());
        self
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

    /// Jump to a step directly (pointer tabs). `step == questions.len()` is
    /// the Submit step; larger values clamp to the last question.
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

    // ── geometry (shared by render and hit regions) ────────────────────────

    /// Row geometry of the active question, mirroring the renderer's line
    /// order exactly. Used by [`Self::component_regions`] so painting and
    /// hit-testing cannot drift.
    fn rows(&self, area: Rect) -> WorkflowRows {
        let inner = prompt_content_area(area);
        let mut rows = WorkflowRows::default();
        if self.confirm_focused {
            rows.submit_y = Some(inner.y.saturating_add(2));
            return rows;
        }
        let Some(q) = self.questions.get(self.active_question_idx) else {
            return rows;
        };
        let mut y = inner.y;
        if self.questions.len() > 1 {
            let mut x = inner.x;
            for (i, question) in self.questions.iter().enumerate() {
                if i > 0 {
                    x = x.saturating_add(3);
                }
                let width = (question.header.chars().count() as u16).saturating_add(2);
                rows.tab_rects.push((i, Rect::new(x, y, width, 1)));
                x = x.saturating_add(width);
            }
            if self.require_confirm {
                let width = 8; // "[Submit]"
                rows.tab_rects.push((
                    self.questions.len(),
                    Rect::new(x.saturating_add(3), y, width, 1),
                ));
            }
            y = y.saturating_add(2);
        }
        // Prompt line + blank spacer.
        y = y.saturating_add(2);
        rows.choice_y = (0..q.choices.len())
            .map(|i| y.saturating_add(i as u16))
            .collect();
        rows
    }

    /// Interactive sub-regions for pointer hit-testing.
    pub fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let rows = self.rows(area);
        let mut out = Vec::new();
        if let Some(y) = rows.submit_y {
            out.push((row_rect(area, y), HitId::Submit));
        }
        for (question, rect) in rows.tab_rects {
            out.push((clamp_rect(rect, area), HitId::Tab(question)));
        }
        for (choice, y) in rows.choice_y.iter().enumerate() {
            out.push((
                row_rect(area, *y),
                HitId::Choice {
                    question: self.active_question_idx,
                    choice,
                },
            ));
        }
        out
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(frame_border_style(true, theme))
            .style(Style::default().bg(theme.bg_elevated));
        // Opaque backdrop: the centered dialog never leaks plane cells or
        // stale glyphs from previous frames.
        frame.render_widget(Clear, area);
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
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.muted)
                };

                let mut spans = vec![
                    Span::styled(
                        prefix,
                        if is_selected {
                            Style::default().fg(theme.accent)
                        } else {
                            Style::default()
                        },
                    ),
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

        lines.push(Line::default());

        // Help line: override (approval shortcut legend) or state-derived.
        let help_txt = if let Some(help) = &self.help_override {
            help.as_str()
        } else if self.confirm_focused {
            "Enter to submit · Tab to cycle · Esc to cancel"
        } else if !self.questions.is_empty() {
            let q = &self.questions[self.active_question_idx];
            if q.is_input_active {
                "Enter to save · Esc to exit editing"
            } else if self.questions.len() > 1 {
                "Enter to select · ↑/↓ choose · Tab switch question · Esc cancel"
            } else {
                "Enter to select · ↑/↓ to navigate · Esc to cancel"
            }
        } else {
            "Esc to cancel"
        };
        lines.push(Line::from(Span::styled(help_txt, hint_style(theme))));

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Geometry shared by [`InteractiveWorkflow::render`] and
/// [`InteractiveWorkflow::component_regions`].
#[derive(Clone, Debug, Default)]
struct WorkflowRows {
    /// (question index, rect) for each tab, plus a trailing Submit tab when
    /// confirmation is required. The submit sentinel is `questions.len()`.
    tab_rects: Vec<(usize, Rect)>,
    /// y of each choice row of the active question.
    choice_y: Vec<u16>,
    /// y of the Confirm action row.
    submit_y: Option<u16>,
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

fn row_rect(area: Rect, y: u16) -> Rect {
    clamp_rect(Rect::new(area.x, y, area.width, 1), area)
}

fn clamp_rect(rect: Rect, area: Rect) -> Rect {
    let x = rect.x.max(area.x);
    let y = rect.y.max(area.y);
    let right = (rect.x + rect.width).min(area.x + area.width);
    let bottom = (rect.y + rect.height).min(area.y + area.height);
    if right <= x || bottom <= y {
        Rect::default()
    } else {
        Rect::new(x, y, right - x, bottom - y)
    }
}
