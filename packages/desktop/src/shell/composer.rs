//! Floating Timeline Composer (D-59 Slice 4, F-43 visual).

use gpui::prelude::*;
use gpui::{
    AnyElement, App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, Window,
    div, px,
};
use gpui_base::input::{InputEvent, TextareaState};
use gpui_base::{InputBase, Textarea};
use island::components::chrome::{
    LINEAR_PROGRESS_HEIGHT_COMPACT, LinearProgress, LinearProgressSize,
};
use island::platform::material::WindowMaterialHost;
use island::theme::{
    RoleAccent, SurfaceRole, TextRole, concentric_radius, elevation_sm, fill, hairline, highlight,
    metrics, text, tokens,
};

pub const MIN_ROWS: usize = 2;
pub const MAX_ROWS: usize = 8;
const LINE_HEIGHT: f32 = 21.0;
/// Outer py 16 + input py 16 + gap 4 + compact_bar 28 + hairline 2.
const VERTICAL_CHROME: f32 = 66.0;
/// Wrapper pb (12) + air above the card (12). Vertical only.
const OUTER_GAP: f32 = 24.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSubmission {
    pub command_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerContext {
    Unknown,
    Fill { used: u64, window: u64 },
}

/// Conservative footprint used as Timeline trailing padding. The last laid
/// out input height captures soft wrapping; explicit lines provide a safe
/// fallback before the first layout.
pub fn footprint_for_text(
    text: &str,
    measured_input_height: f32,
    measured_card_height: f32,
) -> f32 {
    let logical_rows = text.lines().count().clamp(MIN_ROWS, MAX_ROWS);
    let input_height = measured_input_height.max(logical_rows as f32 * LINE_HEIGHT);
    let chrome = if measured_card_height > 0.0 {
        (measured_card_height - measured_input_height).max(0.0)
    } else {
        VERTICAL_CHROME
    };
    input_height + chrome + OUTER_GAP
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

pub fn format_token_count(tokens: u64) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}

pub type ComposerAction = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

pub struct ComposerView {
    pub input: Entity<TextareaState>,
    pub material: WindowMaterialHost,
    pub enabled: bool,
    pub running: bool,
    pub pending: bool,
    pub context: ComposerContext,
    pub error: Option<String>,
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
        let error = self.error.clone();

        div()
            .id("piko-composer-float")
            .absolute()
            .bottom_0()
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .px(m.space_lg)
            .pb(m.space_md)
            .cursor(CursorStyle::IBeam)
            .child(
                div()
                    .id("piko-composer")
                    .w_full()
                    .max_w(m.reading_width)
                    .flex()
                    .flex_col()
                    .gap(m.space_xs)
                    .px(m.space_sm)
                    .py(m.space_sm)
                    .rounded(m.island_radius)
                    .overflow_hidden()
                    .shadow(elevation_sm().box_shadow())
                    .border_1()
                    .border_color(hairline(SurfaceRole::Elevated))
                    .bg(fill(SurfaceRole::Elevated, self.material))
                    .text_color(t.fg_rgba())
                    .cursor(CursorStyle::IBeam)
                    .when_some(error, |composer, error| {
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
                            .py(px(8.))
                            .rounded(concentric_radius(m.island_radius, m.space_sm))
                            .bg(fill(SurfaceRole::Content, self.material))
                            .text_color(t.fg_rgba())
                            .cursor(CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                input_for_focus.update(cx, |state, cx| state.focus(window, cx));
                            })
                            .child(Textarea::new(&self.input)),
                    )
                    .child(self.render_action_row(submit, cancel)),
            )
            .into_any_element()
    }

    fn render_action_row(self, submit: ComposerAction, cancel: ComposerAction) -> impl IntoElement {
        let t = tokens();
        let m = metrics();
        div()
            .flex()
            .items_center()
            .gap(m.space_sm)
            .when_some(
                match self.context {
                    ComposerContext::Unknown => None,
                    ComposerContext::Fill { used, window } => Some((used, window)),
                },
                |row, (used, window)| {
                    let fraction = if window == 0 {
                        0.0
                    } else {
                        (used as f32 / window as f32).clamp(0.0, 1.0)
                    };
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(m.space_xs)
                            .child(
                                div()
                                    .w(px(72.))
                                    .h(px(LINEAR_PROGRESS_HEIGHT_COMPACT))
                                    .child(
                                        LinearProgress::new("piko-context-meter")
                                            .size(LinearProgressSize::Compact)
                                            .value(Some(fraction)),
                                    ),
                            )
                            .child(text(TextRole::Meta).text_color(t.muted_fg_rgba()).child(
                                format!(
                                    "{}/{}",
                                    format_token_count(used),
                                    format_token_count(window)
                                ),
                            )),
                    )
                },
            )
            .child(div().flex_1())
            .when(self.running, |bar| {
                bar.child(action_button("Cancel", true, cancel))
            })
            .child(action_button(
                if self.pending { "Sending…" } else { "Send" },
                self.enabled && !self.pending,
                submit,
            ))
    }
}

fn action_button(label: &'static str, enabled: bool, action: ComposerAction) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(label)
        .px(m.space_sm)
        .py(px(3.))
        .rounded(concentric_radius(m.island_radius, m.space_sm))
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
    fn two_row_identity_is_one_hundred_thirty_two() {
        assert_eq!(footprint_for_text("one", 0.0, 0.0), 132.0);
    }

    #[test]
    fn footprint_grows_then_clamps() {
        let short = footprint_for_text("one", 0.0, 0.0);
        let four = footprint_for_text("1\n2\n3\n4", 0.0, 0.0);
        let many = footprint_for_text(&["x"; 20].join("\n"), 0.0, 0.0);
        let eight = footprint_for_text(&["x"; MAX_ROWS].join("\n"), 0.0, 0.0);
        assert!(four > short);
        assert_eq!(many, eight);
    }

    #[test]
    fn measured_soft_wrap_height_expands_timeline_clearance() {
        let logical_only = footprint_for_text("one very long logical line", 0.0, 0.0);
        let wrapped = footprint_for_text("one very long logical line", 150.0, 0.0);
        assert!(wrapped > logical_only);
    }

    #[test]
    fn measured_card_overrides_fallback_chrome() {
        let fallback = footprint_for_text("one", 42.0, 0.0);
        let measured = footprint_for_text("one", 42.0, 120.0);
        assert_eq!(fallback, 42.0 + 66.0 + 24.0);
        assert_eq!(measured, 42.0 + (120.0 - 42.0) + 24.0);
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

    #[test]
    fn token_counts_compact_thousands() {
        assert_eq!(format_token_count(12_000), "12k");
        assert_eq!(format_token_count(128_000), "128k");
        assert_eq!(format_token_count(400), "400");
    }
}
