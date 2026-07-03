use std::collections::VecDeque;

use piko_protocol::{
    InteractionAnswer, InteractionId, InteractionQuestion, UserInteractionResponse,
};
use ratatui::{Frame, layout::Rect};

use crate::{
    theme::Theme,
    ui::components::interactive_workflow::{ChoiceOption, InteractiveWorkflow, Question},
};

pub struct PendingInteraction {
    pub id: InteractionId,
    pub questions: Vec<InteractionQuestion>,
    pub workflow: InteractiveWorkflow,
    pub submitting: bool,
}

pub struct ToolInteractionPanel {
    pending: VecDeque<PendingInteraction>,
}

impl ToolInteractionPanel {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn push(
        &mut self,
        id: InteractionId,
        _title: Option<String>,
        questions: Vec<InteractionQuestion>,
        require_confirm: bool,
    ) {
        let workflow_questions = questions
            .iter()
            .map(|question| {
                Question::new(
                    question.header.clone(),
                    question.prompt.clone(),
                    question
                        .choices
                        .iter()
                        .map(|choice| ChoiceOption {
                            label: choice.label.clone(),
                            has_input: choice.input.is_some(),
                            input_prompt: choice
                                .input
                                .as_ref()
                                .map(|input| input.prompt.clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                )
            })
            .collect();
        self.pending.push_back(PendingInteraction {
            id,
            questions,
            workflow: InteractiveWorkflow::new(workflow_questions, require_confirm),
            submitting: false,
        });
    }

    pub fn resolve(&mut self, id: &str) {
        self.pending.retain(|interaction| interaction.id != id);
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn front_mut(&mut self) -> Option<&mut PendingInteraction> {
        self.pending.front_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        let Some(interaction) = self.pending.front() else {
            return;
        };
        interaction.workflow.render(frame, area, theme);
    }

    pub fn submit_response(&mut self) -> Option<(InteractionId, UserInteractionResponse)> {
        let interaction = self.pending.front_mut()?;
        if !interaction.workflow.can_submit() {
            return None;
        }
        let answers = interaction
            .workflow
            .selected_answers()
            .into_iter()
            .filter_map(|(question_idx, choice_idx, input)| {
                let question = interaction.questions.get(question_idx)?;
                let choice = question.choices.get(choice_idx)?;
                Some(InteractionAnswer {
                    question_id: question.id.clone(),
                    choice_id: choice.id.clone(),
                    value: choice.value.clone(),
                    input,
                })
            })
            .collect();
        interaction.submitting = true;
        Some((
            interaction.id.clone(),
            UserInteractionResponse::Submit { answers },
        ))
    }

    pub fn cancel_response(&mut self) -> Option<(InteractionId, UserInteractionResponse)> {
        let interaction = self.pending.front_mut()?;
        interaction.submitting = true;
        Some((
            interaction.id.clone(),
            UserInteractionResponse::Cancel {
                reason: Some("User cancelled".into()),
            },
        ))
    }
}
