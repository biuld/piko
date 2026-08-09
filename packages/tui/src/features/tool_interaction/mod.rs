use std::collections::VecDeque;

use piko_protocol::{
    InteractionAnswer, InteractionId, InteractionQuestion, UserInteractionResponse,
};
use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{Frame, layout::Rect};

use crate::app::{HitId, command::ToolInteractionAction};
use crate::navigation::SurfaceId;
use crate::ui::components::pane::PaneTitleAffix;
use crate::ui::interaction::{ComponentHit, PointerComponent, PointerGesture};
use crate::{
    theme::Theme,
    ui::components::interactive_workflow::{ChoiceOption, InteractiveWorkflow, Question},
};

pub struct PendingInteraction {
    pub id: InteractionId,
    /// Agent instance that requested the interaction; F-22 foreground key.
    pub agent_instance_id: String,
    pub questions: Vec<InteractionQuestion>,
    pub workflow: InteractiveWorkflow,
    pub submitting: bool,
    /// True when this interaction owns the ToolInteraction surface (it was
    /// surfaced to the user). Auto-resolving interactions never surface.
    pub surfaced: bool,
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
        agent_instance_id: String,
        _title: Option<String>,
        questions: Vec<InteractionQuestion>,
        require_confirm: bool,
        surfaced: bool,
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
            agent_instance_id,
            questions,
            workflow: InteractiveWorkflow::new(workflow_questions, require_confirm),
            submitting: false,
            surfaced,
        });
    }

    /// True when any pending interaction is attributed to `agent_instance_id`.
    pub fn pending_for_agent(&self, agent_instance_id: &str) -> bool {
        self.pending
            .iter()
            .any(|i| i.agent_instance_id == agent_instance_id)
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

    pub fn front(&self) -> Option<&PendingInteraction> {
        self.pending.front()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        theme: &Theme,
        interaction_state: InteractionState<HitId>,
    ) {
        let Some(interaction) = self.pending.front() else {
            return;
        };
        let affixes = if self.pending.len() > 1 {
            vec![PaneTitleAffix::selection(1, self.pending.len())]
        } else {
            Vec::new()
        };
        interaction.workflow.render_modal(
            frame,
            area,
            theme,
            "Tool Interaction",
            affixes,
            interaction_state,
        );
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

impl Component<HitId, Theme> for ToolInteractionPanel {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &Theme) {
        self.render(frame, area, ctx, InteractionState::default());
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &Theme,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx, interaction);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.pending
            .front()
            .map(|interaction| interaction.workflow.component_regions_modal(area))
            .unwrap_or_default()
    }
}

impl SurfacePanel<SurfaceId, HitId, Theme> for ToolInteractionPanel {
    fn region(&self) -> SurfaceId {
        SurfaceId::ToolInteraction
    }
}

impl PointerComponent<HitId> for ToolInteractionPanel {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        if gesture != PointerGesture::Activate {
            return Vec::new();
        }
        match hit.element {
            Some(HitId::TextInput) => {
                if let Some(interaction) = self.pending.front_mut() {
                    interaction
                        .workflow
                        .move_active_input_to_column(hit.local_x());
                }
                Vec::new()
            }
            Some(HitId::Choice { choice, .. }) => vec![
                ToolInteractionAction::Choice(choice).into(),
                ToolInteractionAction::Submit.into(),
            ],
            Some(HitId::Tab(step)) => vec![ToolInteractionAction::GotoStep(step).into()],
            Some(HitId::Submit) => vec![ToolInteractionAction::Submit.into()],
            _ => Vec::new(),
        }
    }
}
