//! Floating Timeline Composer (D-59 Slice 4).

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Entity, InteractiveElement, IntoElement, MouseButton, Window, div, px,
};
use gpui_base::input::{InputEvent, TextareaState};
use gpui_base::{InputBase, Textarea};
use island::platform::material::WindowMaterialHost;
use island::theme::{
    RoleAccent, SurfaceRole, TextRole, fill, hairline, highlight, metrics, text, tokens,
};

pub const MIN_ROWS: usize = 2;
pub const MAX_ROWS: usize = 8;
const LINE_HEIGHT: f32 = 21.0;
const VERTICAL_CHROME: f32 = 58.0;
const OUTER_GAP: f32 = 24.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSubmission {
    pub command_id: String,
    pub text: String,
}

/// Conservative footprint used as Timeline trailing padding. The last laid
/// out input height captures soft wrapping; explicit lines provide a safe
/// fallback before the first layout.
pub fn footprint_for_text(text: &str, measured_input_height: f32, return_to_latest: bool) -> f32 {
    let logical_rows = text.lines().count().clamp(MIN_ROWS, MAX_ROWS);
    let input_height = measured_input_height.max(logical_rows as f32 * LINE_HEIGHT);
    let latest = if return_to_latest { 28.0 } else { 0.0 };
    input_height + VERTICAL_CHROME + OUTER_GAP + latest
}

pub fn new_input(window: &mut Window, cx: &mut App) -> Entity<TextareaState> {
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(MIN_ROWS, MAX_ROWS)
            .submit_on_enter(true)
            .placeholder("Message the selected agent…")
    })
}

pub fn is_submit_event(event: &InputEvent) -> bool {
    matches!(event, InputEvent::PressEnter { shift: false, .. })
}

pub fn should_clear_accepted_draft(current: &str, submitted: &str) -> bool {
    current.trim() == submitted
}

pub type ComposerAction = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

pub struct ComposerView {
    pub input: Entity<TextareaState>,
    pub material: WindowMaterialHost,
    pub enabled: bool,
    pub running: bool,
    pub pending: bool,
    pub model: String,
    pub thinking: String,
    pub context: String,
    pub error: Option<String>,
    pub on_model: ComposerAction,
    pub on_thinking: ComposerAction,
    pub on_submit: ComposerAction,
    pub on_cancel: ComposerAction,
}

impl ComposerView {
    pub fn render(self) -> AnyElement {
        let t = tokens();
        let m = metrics();
        let input_for_focus = self.input.clone();
        let submit = self.on_submit.clone();
        let cancel = self.on_cancel.clone();

        div()
            .id("piko-composer-float")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .px(px(18.))
            .pb(px(12.))
            .child(
                div()
                    .id("piko-composer")
                    .w_full()
                    .max_w(px(820.))
                    .flex()
                    .flex_col()
                    .gap(m.space_xs)
                    .px(m.space_sm)
                    .py(m.space_sm)
                    .rounded_md()
                    .border_1()
                    .border_color(hairline(SurfaceRole::Elevated))
                    .bg(fill(SurfaceRole::Elevated, self.material))
                    .text_color(t.fg_rgba())
                    .when_some(self.error, |composer, error| {
                        composer.child(
                            text(TextRole::Meta)
                                .text_color(t.role_accent(RoleAccent::Danger))
                                .child(error),
                        )
                    })
                    .child(
                        InputBase::new("piko-composer-input")
                            .w_full()
                            .min_h(px(MIN_ROWS as f32 * LINE_HEIGHT))
                            .max_h(px(MAX_ROWS as f32 * LINE_HEIGHT + 12.0))
                            .px(m.space_sm)
                            .py(px(6.))
                            .rounded_sm()
                            .border_1()
                            .border_color(hairline(SurfaceRole::Content))
                            .bg(fill(SurfaceRole::Content, self.material))
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                input_for_focus.update(cx, |state, cx| state.focus(window, cx));
                            })
                            .child(Textarea::new(&self.input)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(m.space_sm)
                            .child(action_chip("composer-model", self.model, self.on_model))
                            .child(action_chip(
                                "composer-thinking",
                                self.thinking,
                                self.on_thinking,
                            ))
                            .child(meta_chip(self.context))
                            .child(div().flex_1())
                            .when(self.running, |bar| {
                                bar.child(action_button("Cancel", true, cancel))
                            })
                            .child(action_button(
                                if self.pending { "Sending…" } else { "Send" },
                                self.enabled && !self.pending,
                                submit,
                            )),
                    ),
            )
            .into_any_element()
    }
}

fn meta_chip(label: String) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .px(m.space_xs)
        .py(px(2.))
        .rounded_sm()
        .bg(highlight())
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(label),
        )
}

fn action_chip(id: &'static str, label: String, action: ComposerAction) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(id)
        .px(m.space_xs)
        .py(px(2.))
        .rounded_sm()
        .bg(highlight())
        .cursor_pointer()
        .hover(|style| style.bg(fill(SurfaceRole::Content, WindowMaterialHost::opaque())))
        .on_click(move |_, window, cx| action(window, cx))
        .child(
            text(TextRole::Meta)
                .text_color(t.muted_fg_rgba())
                .child(label),
        )
}

fn action_button(label: &'static str, enabled: bool, action: ComposerAction) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(label)
        .px(m.space_sm)
        .py(px(3.))
        .rounded_sm()
        .border_1()
        .border_color(hairline(SurfaceRole::Chrome))
        .text_color(if enabled {
            t.fg_rgba()
        } else {
            t.muted_fg_rgba()
        })
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(highlight()))
                .on_click(move |_, window, cx| action(window, cx))
        })
        .child(text(TextRole::Label).child(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_grows_then_clamps_and_reserves_latest_action() {
        let short = footprint_for_text("one", 0.0, false);
        let four = footprint_for_text("1\n2\n3\n4", 0.0, false);
        let many = footprint_for_text(&["x"; 20].join("\n"), 0.0, false);
        let eight = footprint_for_text(&["x"; MAX_ROWS].join("\n"), 0.0, false);
        assert!(four > short);
        assert_eq!(many, eight);
        assert_eq!(footprint_for_text("one", 0.0, true), short + 28.0);
    }

    #[test]
    fn measured_soft_wrap_height_expands_timeline_clearance() {
        let logical_only = footprint_for_text("one very long logical line", 0.0, false);
        let wrapped = footprint_for_text("one very long logical line", 150.0, false);
        assert!(wrapped > logical_only);
    }

    #[test]
    fn only_plain_enter_submits() {
        assert!(is_submit_event(&InputEvent::PressEnter {
            secondary: false,
            shift: false,
        }));
        assert!(!is_submit_event(&InputEvent::PressEnter {
            secondary: false,
            shift: true,
        }));
    }

    #[test]
    fn accepted_submit_never_clears_a_newer_edit() {
        assert!(should_clear_accepted_draft("  sent text  ", "sent text"));
        assert!(!should_clear_accepted_draft(
            "sent text plus a new edit",
            "sent text"
        ));
    }
}
