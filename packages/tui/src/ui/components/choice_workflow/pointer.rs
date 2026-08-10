use piko_tui_layout::InteractionState;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
};
use unicode_width::UnicodeWidthStr;

use super::{ChoiceWorkflow, clamp_rect, row_rect};
use crate::{
    app::HitId,
    theme::Theme,
    ui::components::{
        feedback::selected_bg, hover_bg, selectable_list::paint_index_hover, selection_prefix,
    },
};

impl ChoiceWorkflow {
    /// Post-paint selected full-row bg + hover preview (content-relative).
    pub(super) fn paint_selected_and_hover(
        &self,
        frame: &mut Frame<'_>,
        content: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        let regions = self.component_regions_in(content, content);
        // Selected full-row background only on choice / confirm rows — not tabs
        // (tabs use accent text). Matches SelectableList row highlight, not a
        // second full-pane fill.
        if let Some(bg) = selected_bg(theme) {
            for (rect, id) in &regions {
                let paint = match id {
                    HitId::Choice { question, choice } => {
                        self.active_question_idx == *question
                            && self
                                .questions
                                .get(*question)
                                .is_some_and(|q| q.selected_idx == *choice)
                    }
                    HitId::Submit => self.confirm_focused,
                    _ => false,
                };
                if paint && !rect.is_empty() {
                    frame.buffer_mut().set_style(*rect, Style::default().bg(bg));
                }
            }
        }
        // Choice hover via shared helper; Tab/Submit keep simple hover.
        let choice_regions: Vec<(Rect, usize)> = regions
            .iter()
            .filter_map(|(rect, id)| match id {
                HitId::Choice { choice, .. } => Some((*rect, *choice)),
                _ => None,
            })
            .collect();
        let selected_choice = self
            .questions
            .get(self.active_question_idx)
            .map(|q| q.selected_idx)
            .unwrap_or(0);
        let hovered_choice = match interaction.hovered {
            Some(HitId::Choice { choice, .. }) => Some(choice),
            _ => None,
        };
        paint_index_hover(
            frame,
            &choice_regions,
            hovered_choice,
            selected_choice,
            theme,
        );

        let Some(element) = interaction.hovered else {
            return;
        };
        if matches!(element, HitId::Choice { .. }) || self.element_is_selected(element) {
            return;
        }
        let Some(background) = hover_bg(theme) else {
            return;
        };
        if let Some((rect, _)) = regions.into_iter().find(|(_, id)| *id == element) {
            frame
                .buffer_mut()
                .set_style(rect, Style::default().bg(background));
        }
    }

    pub fn render_embedded_hover(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        // Embedded: `area` is already the Pane footer/content rect.
        self.paint_selected_and_hover(frame, area, theme, interaction);
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

    /// Hit regions for a standalone Decide dock (host = full pane area).
    pub fn component_regions_modal(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let inner = self.modal_content_area(area);
        self.component_regions_in(inner, area)
    }

    /// Hit regions when body fills `area` (parent Pane content / footer).
    pub fn component_regions_embedded(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.component_regions_in(area, area)
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
        let prefix_w = UnicodeWidthStr::width(selection_prefix(true).as_str()) as u16;
        let x = inner
            .x
            .saturating_add(prefix_w)
            .saturating_add(UnicodeWidthStr::width(number.as_str()) as u16)
            .saturating_add(UnicodeWidthStr::width(label.as_str()) as u16)
            .saturating_add(2); // ": "
        Some(Position::new(x, y))
    }
}
