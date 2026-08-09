use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
};
use unicode_width::UnicodeWidthStr;

use super::{InteractiveWorkflow, clamp_rect, prompt_content_area, row_rect};
use crate::{app::HitId, theme::Theme, ui::components::hover_bg};

impl InteractiveWorkflow {
    pub(super) fn render_hover(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.paint_hover(frame, area, theme, interaction, false);
    }

    pub fn render_embedded_hover(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.paint_hover(frame, area, theme, interaction, true);
    }

    fn paint_hover(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
        embedded: bool,
    ) {
        let Some(element) = interaction.hovered else {
            return;
        };
        if self.element_is_selected(element) {
            return;
        }
        let Some(background) = hover_bg(theme) else {
            return;
        };
        let regions = if embedded {
            self.component_regions_embedded(area)
        } else {
            self.component_regions_modal(area)
        };
        if let Some((rect, _)) = regions.into_iter().find(|(_, id)| *id == element) {
            frame
                .buffer_mut()
                .set_style(rect, Style::default().bg(background));
        }
    }

    fn element_is_selected(&self, element: HitId) -> bool {
        match element {
            HitId::Choice { question, choice } => {
                self.active_question_idx == question
                    && self
                        .questions
                        .get(question)
                        .is_some_and(|q| q.selected_idx == choice)
            }
            HitId::Tab(step) => !self.confirm_focused && self.active_question_idx == step,
            HitId::Submit => self.confirm_focused,
            _ => false,
        }
    }

    pub fn component_regions_modal(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let inner = self.modal_content_area(area);
        self.component_regions_in(inner, area)
    }

    pub fn component_regions_embedded(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let inner = prompt_content_area(area);
        self.component_regions_in(inner, area)
    }

    fn component_regions_in(&self, inner: Rect, bounds: Rect) -> Vec<(Rect, HitId)> {
        let rows = self.rows_in(inner);
        let mut out = Vec::new();
        if let Some(y) = rows.submit_y {
            out.push((row_rect(inner, y), HitId::Submit));
        }
        for (question, rect) in rows.tab_rects {
            out.push((clamp_rect(rect, bounds), HitId::Tab(question)));
        }
        for (choice, y) in rows.choice_y.iter().enumerate() {
            out.push((
                row_rect(inner, *y),
                HitId::Choice {
                    question: self.active_question_idx,
                    choice,
                },
            ));
        }
        if let Some(origin) = self.input_field_origin_in(inner) {
            out.push((
                Rect::new(
                    origin.x,
                    origin.y,
                    inner.x.saturating_add(inner.width).saturating_sub(origin.x),
                    1,
                ),
                HitId::TextInput,
            ));
        }
        out
    }

    pub fn move_active_input_to_column(&mut self, column: u16) {
        if let Some(question) = self.questions.get_mut(self.active_question_idx)
            && question.is_input_active
        {
            question.input_value.move_to_column(column);
        }
    }

    pub fn input_cursor(&self, area: Rect) -> Option<Position> {
        let origin = self.input_field_origin(area)?;
        let question = &self.questions[self.active_question_idx];
        Some(question.input_value.caret_position(origin))
    }

    fn input_field_origin(&self, area: Rect) -> Option<Position> {
        self.input_field_origin_in(self.modal_content_area(area))
    }

    fn input_field_origin_in(&self, inner: Rect) -> Option<Position> {
        let question = self.questions.get(self.active_question_idx)?;
        if !question.is_input_active {
            return None;
        }
        let choice = question.selected_idx;
        let rows = self.rows_in(inner);
        let y = *rows.choice_y.get(choice)?;
        let label = &question.choices.get(choice)?.label;
        let number = format!("{}. ", choice + 1);
        let x = inner
            .x
            .saturating_add(2)
            .saturating_add(UnicodeWidthStr::width(number.as_str()) as u16)
            .saturating_add(UnicodeWidthStr::width(label.as_str()) as u16)
            .saturating_add(2);
        Some(Position::new(x, y))
    }
}
