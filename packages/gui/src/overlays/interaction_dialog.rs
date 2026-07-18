//! Structured user-interaction dialog with explicit choices and inline input.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::Disableable;
use gpui_component::WindowExt;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use piko_client_core::state::PendingInteraction;
use piko_protocol::{InteractionAnswer, UserInteractionResponse};

type RespondFn = Rc<dyn Fn(UserInteractionResponse, &mut Window, &mut App) + 'static>;

struct InteractionForm {
    interaction: PendingInteraction,
    selected_choices: Vec<Option<usize>>,
    inputs: Vec<Option<Entity<InputState>>>,
    on_respond: RespondFn,
}

impl InteractionForm {
    fn answers(&self, cx: &App) -> Vec<InteractionAnswer> {
        self.selected_choices
            .iter()
            .enumerate()
            .filter_map(|(question_ix, selected)| {
                let choice_ix = selected.as_ref().copied()?;
                let question = &self.interaction.questions[question_ix];
                let choice = &question.choices[choice_ix];
                let input = choice.input.as_ref().and_then(|_| {
                    self.inputs[question_ix]
                        .as_ref()
                        .map(|input| input.read(cx).value().to_string())
                });
                answer_for_selection(question, choice_ix, input)
            })
            .collect()
    }
}

fn answer_for_selection(
    question: &piko_protocol::InteractionQuestion,
    choice_ix: usize,
    input: Option<String>,
) -> Option<InteractionAnswer> {
    let choice = question.choices.get(choice_ix)?;
    Some(InteractionAnswer {
        question_id: question.id.clone(),
        choice_id: choice.id.clone(),
        value: choice.value.clone(),
        input: choice.input.as_ref().and(input),
    })
}

impl Render for InteractionForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        let mut question_elements = Vec::new();

        for (question_ix, question) in self.interaction.questions.iter().enumerate() {
            let selected = self.selected_choices[question_ix];
            let mut choices = Vec::new();
            for (choice_ix, choice) in question.choices.iter().enumerate() {
                let entity = entity.clone();
                choices.push(
                    Button::new(SharedString::from(format!("q-{question_ix}-c-{choice_ix}")))
                        .label(choice.label.clone())
                        .when(selected == Some(choice_ix), |button| button.primary())
                        .disabled(self.interaction.response_in_flight)
                        .on_click(move |_, _, cx| {
                            if let Some(form) = entity.upgrade() {
                                form.update(cx, |form, cx| {
                                    form.selected_choices[question_ix] = Some(choice_ix);
                                    cx.notify();
                                });
                            }
                        }),
                );
            }

            let selected_input = selected
                .and_then(|choice_ix| question.choices.get(choice_ix))
                .and_then(|choice| choice.input.as_ref());
            let mut section = div()
                .id(SharedString::from(format!("question-{question_ix}")))
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(
                    if question.header.is_empty() {
                        question.prompt.clone()
                    } else {
                        question.header.clone()
                    },
                ))
                .when(
                    !question.header.is_empty() && !question.prompt.is_empty(),
                    |d| d.child(div().text_xs().child(question.prompt.clone())),
                )
                .child(div().flex().flex_wrap().gap_2().children(choices));

            if let (Some(input_spec), Some(input)) =
                (selected_input, self.inputs[question_ix].as_ref())
            {
                section = section
                    .child(div().text_xs().child(input_spec.prompt.clone()))
                    .child(Input::new(input));
            }
            question_elements.push(section);
        }

        let can_submit = !self.interaction.response_in_flight
            && self
                .interaction
                .questions
                .iter()
                .enumerate()
                .all(|(ix, question)| !question.required || self.selected_choices[ix].is_some());
        let on_submit = self.on_respond.clone();
        let on_cancel = self.on_respond.clone();
        let entity_for_submit = entity.clone();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(question_elements)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("ix-submit")
                            .primary()
                            .label("Submit")
                            .disabled(!can_submit)
                            .on_click(move |_, window, cx| {
                                if let Some(form) = entity_for_submit.upgrade() {
                                    let answers = form.read(cx).answers(cx);
                                    on_submit(
                                        UserInteractionResponse::Submit { answers },
                                        window,
                                        cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        Button::new("ix-cancel")
                            .label("Cancel")
                            .disabled(self.interaction.response_in_flight)
                            .on_click(move |_, window, cx| {
                                on_cancel(
                                    UserInteractionResponse::Cancel {
                                        reason: Some("user cancelled".into()),
                                    },
                                    window,
                                    cx,
                                );
                            }),
                    ),
            )
    }
}

/// Open the interaction dialog. Cancel sends an explicit host response and the
/// dialog remains until host resolution or reconcile.
pub fn open_interaction_dialog(
    window: &mut Window,
    cx: &mut App,
    interaction: PendingInteraction,
    remaining: usize,
    on_respond: RespondFn,
) {
    let inputs = interaction
        .questions
        .iter()
        .map(|question| {
            question
                .choices
                .iter()
                .find_map(|choice| choice.input.as_ref())
                .map(|input| {
                    cx.new(|cx| {
                        let mut state = InputState::new(window, cx);
                        if let Some(placeholder) = &input.placeholder {
                            state = state.placeholder(placeholder.clone());
                        }
                        state
                    })
                })
        })
        .collect();
    let selected_choices = vec![None; interaction.questions.len()];
    let form = cx.new(|_| InteractionForm {
        interaction: interaction.clone(),
        selected_choices,
        inputs,
        on_respond: on_respond.clone(),
    });
    let title = if remaining > 1 {
        format!("Questions ({remaining} pending)")
    } else {
        "Questions".into()
    };
    let in_flight = interaction.response_in_flight;

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let on_respond = on_respond.clone();
        dialog
            .title(title.clone())
            .overlay_closable(false)
            .close_button(false)
            .keyboard(true)
            .on_cancel(move |_, window, cx| {
                if !in_flight {
                    on_respond(
                        UserInteractionResponse::Cancel {
                            reason: Some("dismissed".into()),
                        },
                        window,
                        cx,
                    );
                }
                false
            })
            .w(px(620.))
            .child(form.clone())
            .footer(|_, _, _, _| Vec::<AnyElement>::new())
    });
}
