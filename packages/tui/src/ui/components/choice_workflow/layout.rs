//! Geometry for ChoiceWorkflow body lines — single source for paint, hit, and
//! input origin. Coordinates are relative to a Pane content (or footer) rect.

use ratatui::layout::Rect;

use super::ChoiceWorkflow;

/// Row geometry of the active view, mirroring [`ChoiceWorkflow::body_lines`]
/// line order exactly.
#[derive(Clone, Debug, Default)]
pub struct WorkflowRows {
    /// (step index, rect) for each tab; Submit step is `questions.len()`.
    pub tab_rects: Vec<(usize, Rect)>,
    /// y of each choice row of the active question.
    pub choice_y: Vec<u16>,
    /// y of the Confirm action row.
    pub submit_y: Option<u16>,
}

impl ChoiceWorkflow {
    /// Layout rows inside `inner` (Pane content / reserved footer).
    pub(super) fn rows_in(&self, inner: Rect) -> WorkflowRows {
        let mut rows = WorkflowRows::default();
        let mut y = inner.y;

        // Multi-question tabs always occupy the first two lines when present
        // (including when confirm is focused — must stay hittable).
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
            y = y.saturating_add(2); // tab row + blank
        }

        if self.confirm_focused {
            // body: "Ready…", blank, "❯ [ Confirm ]"
            y = y.saturating_add(2);
            rows.submit_y = Some(y);
            return rows;
        }

        let Some(q) = self.questions.get(self.active_question_idx) else {
            return rows;
        };
        // Prompt line + blank spacer.
        y = y.saturating_add(2);
        rows.choice_y = (0..q.choices.len())
            .map(|i| y.saturating_add(i as u16))
            .collect();
        rows
    }
}

pub fn row_rect(area: Rect, y: u16) -> Rect {
    clamp_rect(Rect::new(area.x, y, area.width, 1), area)
}

pub fn clamp_rect(rect: Rect, area: Rect) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::components::choice_workflow::{ChoiceOption, Question};

    fn multi_confirm() -> ChoiceWorkflow {
        ChoiceWorkflow::new(
            vec![
                Question::new(
                    "A",
                    "q1",
                    vec![ChoiceOption {
                        label: "x".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    }],
                ),
                Question::new(
                    "B",
                    "q2",
                    vec![ChoiceOption {
                        label: "y".into(),
                        has_input: false,
                        input_prompt: String::new(),
                    }],
                ),
            ],
            true,
        )
    }

    #[test]
    fn confirm_focused_keeps_tabs_and_places_submit_after_ready_block() {
        let mut wf = multi_confirm();
        wf.confirm_focused = true;
        let inner = Rect::new(2, 10, 40, 12);
        let rows = wf.rows_in(inner);
        assert!(
            !rows.tab_rects.is_empty(),
            "tabs must remain hittable while confirm is focused"
        );
        // tabs@10 + blank@11 + Ready@12 + blank@13 + Confirm@14
        assert_eq!(rows.submit_y, Some(14));
    }
}
