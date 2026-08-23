//! Floating two-tone composer with attachment chips (F-47, D-64).

use gpui::prelude::*;
use gpui::{
    AnyElement, App, CursorStyle, Entity, IntoElement, ParentElement, Styled, Window, div, px,
};
use island::components::chrome::{
    LINEAR_PROGRESS_HEIGHT_COMPACT, LinearProgress, LinearProgressSize,
};
use island::components::form::{InputEvent, TextAreaField, TextAreaState};
use island::platform::material::WindowMaterialHost;
use island::theme::{
    IslandTokens, RoleAccent, SurfaceRole, TextRole, elevation_sm, fill, highlight, metrics, text,
    tokens,
};
use piko_protocol::{ContentBlock, MessageContent};

pub const MIN_ROWS: usize = 2;
pub const MAX_ROWS: usize = 8;
pub const HEADER_HEIGHT: f32 = 34.0;
const LINE_HEIGHT: f32 = 21.0;
/// Non-input chrome: visible header band (34) + body top/bottom padding (16)
/// + input↔action gap (4) + action bar (24). Input height is added separately.
const VERTICAL_CHROME: f32 = 78.0;
/// Air between the last timeline row and the composer card.
const OUTER_GAP: f32 = 20.0;
/// InputBase renders the textarea with 8px vertical padding top and bottom.
const INPUT_PADDING: f32 = 16.0;

#[derive(Debug, Clone, PartialEq)]
pub struct PendingSubmission {
    pub command_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerContext {
    Unknown,
    Fill { used: u64, window: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
    File {
        path: String,
    },
    Image {
        path: String,
        data: String,
        mime_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub kind: AttachmentKind,
}

impl Attachment {
    pub fn label(&self) -> String {
        let path = match &self.kind {
            AttachmentKind::File { path } => path,
            AttachmentKind::Image { path, .. } => path,
        };
        std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone())
    }

    pub fn is_image(&self) -> bool {
        matches!(self.kind, AttachmentKind::Image { .. })
    }
}

fn mime_for_extension(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Classify picked paths into file-reference or image attachments. Image
/// bytes are read and base64-encoded eagerly so submit cannot fail late;
/// read failures come back as composer errors.
pub fn classify_paths(paths: &[std::path::PathBuf]) -> (Vec<Attachment>, Vec<String>) {
    let mut attachments = Vec::new();
    let mut errors = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let id = format!("att-{index}-{}", attachments.len());
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Some(mime_type) = mime_for_extension(&extension) {
            match std::fs::read(path) {
                Ok(bytes) => {
                    use base64::Engine as _;
                    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
                    attachments.push(Attachment {
                        id,
                        kind: AttachmentKind::Image {
                            path: path.to_string_lossy().into_owned(),
                            data,
                            mime_type: mime_type.to_string(),
                        },
                    });
                }
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        } else {
            attachments.push(Attachment {
                id,
                kind: AttachmentKind::File {
                    path: path.to_string_lossy().into_owned(),
                },
            });
        }
    }
    (attachments, errors)
}

/// Build the multimodal submission content. `None` means "no attachments":
/// the plain `SubmitTurn { text }` path applies.
pub fn build_submission(text: &str, attachments: &[Attachment]) -> Option<MessageContent> {
    if attachments.is_empty() {
        return None;
    }
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: text.to_string(),
        });
    }
    let files: Vec<&Attachment> = attachments
        .iter()
        .filter(|attachment| matches!(attachment.kind, AttachmentKind::File { .. }))
        .collect();
    if !files.is_empty() {
        let mentions = files
            .iter()
            .map(|attachment| match &attachment.kind {
                AttachmentKind::File { path } => format!("@{path}"),
                _ => unreachable!("filtered above"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(ContentBlock::Text { text: mentions });
    }
    for attachment in attachments {
        if let AttachmentKind::Image {
            data, mime_type, ..
        } = &attachment.kind
        {
            blocks.push(ContentBlock::Image {
                data: data.clone(),
                mime_type: mime_type.clone(),
            });
        }
    }
    (!blocks.is_empty()).then_some(MessageContent::Blocks(blocks))
}

/// The input box height for a draft: display rows (each `\n` segment) clamped
/// to the auto-grow range, plus the field's vertical padding. Used both to size
/// the input and to reserve the Timeline footprint so the two always agree.
pub fn input_box_height(text: &str) -> f32 {
    let rows = text.split('\n').count().clamp(MIN_ROWS, MAX_ROWS);
    rows as f32 * LINE_HEIGHT + INPUT_PADDING
}

/// Conservative footprint used as Timeline trailing padding.
pub fn footprint_for_text(text: &str) -> f32 {
    input_box_height(text) + VERTICAL_CHROME + OUTER_GAP
}

pub fn new_input(window: &mut Window, cx: &mut App) -> Entity<TextAreaState> {
    cx.new(|cx| {
        TextAreaState::new(window, cx)
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
pub type RemoveAttachmentAction = std::rc::Rc<dyn Fn(String, &mut Window, &mut App)>;

pub struct ComposerView {
    pub input: Entity<TextAreaState>,
    /// Pre-computed input box height, matching the Timeline footprint.
    pub input_height: f32,
    pub material: WindowMaterialHost,
    pub enabled: bool,
    pub running: bool,
    pub pending: bool,
    pub context: ComposerContext,
    pub error: Option<String>,
    pub attachments: Vec<Attachment>,
    pub model_button: AnyElement,
    pub thinking_button: AnyElement,
    pub latest_button: Option<AnyElement>,
    pub on_submit: ComposerAction,
    pub on_cancel: ComposerAction,
    pub on_attach: ComposerAction,
    pub on_remove_attachment: RemoveAttachmentAction,
}

impl ComposerView {
    pub fn render(self) -> AnyElement {
        let t = tokens();
        let m = metrics();
        let Self {
            input,
            input_height,
            material,
            enabled,
            running,
            pending,
            context,
            error,
            attachments,
            model_button,
            thinking_button,
            latest_button,
            on_submit,
            on_cancel,
            on_attach,
            on_remove_attachment,
        } = self;

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
                    .text_color(t.fg_rgba())
                    // Header: a full-width darker band tucked behind the body.
                    // Top corners are rounded like the card; the bottom stays
                    // straight and bleeds below the body so the body's full
                    // rounded rectangle—drawn on top—covers it, and the seam
                    // follows the body's top corners without a gap.
                    .child(
                        div()
                            .id("piko-composer-header")
                            .h(px(HEADER_HEIGHT) + m.island_radius)
                            .flex()
                            .items_center()
                            .gap(m.space_sm)
                            .px(m.space_sm)
                            .pb(m.island_radius)
                            .rounded_t(m.island_radius)
                            .bg(fill(SurfaceRole::Content, self.material))
                            .child(model_button)
                            .when_some(latest_button, |header, latest| header.child(latest))
                            .child(div().flex_1())
                            .when_some(
                                match context {
                                    ComposerContext::Unknown => None,
                                    ComposerContext::Fill { used, window } => Some((used, window)),
                                },
                                |header, (used, window)| {
                                    header.child(Self::context_meter(used, window))
                                },
                            )
                            .child(thinking_button),
                    )
                    // Body: the single-color Elevated card. Its full rounded
                    // rectangle is drawn above the header, overlapping the
                    // header's bottom so the transition follows the body's top
                    // corners.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(m.space_xs)
                            .px(m.space_sm)
                            .py(m.space_sm)
                            .mt(-m.island_radius)
                            .rounded(m.island_radius)
                            .bg(fill(SurfaceRole::Elevated, self.material))
                            .shadow(elevation_sm().box_shadow())
                            .when_some(error, |composer, error| {
                                composer.child(
                                    text(TextRole::Meta)
                                        .text_color(t.role_accent(RoleAccent::Danger))
                                        .child(error),
                                )
                            })
                            .child(
                                TextAreaField::new("piko-composer-input", &input)
                                    .bare()
                                    .material(self.material)
                                    .height_range(px(input_height), px(input_height))
                                    .radius(m.radius_sm),
                            )
                            .children(Self::render_attachment_chips(
                                &attachments,
                                material,
                                on_remove_attachment,
                            ))
                            .children(Some(Self::render_action_row(
                                enabled, running, pending, on_cancel, on_attach, on_submit,
                            ))),
                    ),
            )
            .into_any_element()
    }

    /// Context progress + token count shown in the composer header's right rail.
    fn context_meter(used: u64, window: u64) -> impl IntoElement {
        let t = tokens();
        let m = metrics();
        let fraction = if window == 0 {
            0.0
        } else {
            (used as f32 / window as f32).clamp(0.0, 1.0)
        };
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
            .child(
                text(TextRole::Meta)
                    .text_color(t.muted_fg_rgba())
                    .child(format!(
                        "{}/{}",
                        format_token_count(used),
                        format_token_count(window)
                    )),
            )
    }

    fn render_attachment_chips(
        attachments: &[Attachment],
        material: WindowMaterialHost,
        on_remove_attachment: RemoveAttachmentAction,
    ) -> Option<AnyElement> {
        if attachments.is_empty() {
            return None;
        }
        let m = metrics();
        let t = tokens();
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(m.space_xs)
            .min_w_0();
        for attachment in attachments.iter().cloned() {
            let label = tool_body_clamp(&attachment.label(), 28);
            let icon = if attachment.is_image() {
                "🖼 "
            } else {
                "📄 "
            };
            let on_remove = on_remove_attachment.clone();
            let id = attachment.id.clone();
            row = row.child(
                div()
                    .id(gpui::SharedString::from(format!("chip-{}", attachment.id)))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(m.space_xs)
                    .py(px(2.))
                    .rounded_full()
                    .bg(fill(SurfaceRole::Content, material))
                    .cursor_pointer()
                    .hover(|style| style.bg(highlight()))
                    .on_click(move |_, window, cx| {
                        on_remove(id.clone(), window, cx);
                    })
                    .child(
                        text(TextRole::Meta)
                            .text_color(t.fg_rgba())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(format!("{icon}{label}")),
                    )
                    .child(
                        text(TextRole::Meta)
                            .text_color(t.muted_fg_rgba())
                            .child("×"),
                    ),
            );
        }
        Some(row.into_any_element())
    }

    fn render_action_row(
        enabled: bool,
        running: bool,
        pending: bool,
        on_cancel: ComposerAction,
        on_attach: ComposerAction,
        on_submit: ComposerAction,
    ) -> impl IntoElement {
        let m = metrics();
        let submit = on_submit;
        let cancel = on_cancel;
        let attach = on_attach;
        div()
            .flex()
            .items_center()
            .gap(m.space_sm)
            .child(action_button("+ Attach", enabled, false, attach))
            .child(div().flex_1())
            .when(running, |bar| {
                bar.child(action_button("Cancel", true, false, cancel))
            })
            .child(action_button(
                if pending { "Sending…" } else { "Send" },
                enabled && !pending,
                true,
                submit,
            ))
    }
}

fn action_button(
    label: &'static str,
    enabled: bool,
    primary: bool,
    action: ComposerAction,
) -> impl IntoElement {
    let t = tokens();
    let m = metrics();
    div()
        .id(gpui::SharedString::from(label))
        .px(m.space_sm)
        .py(px(3.))
        .rounded(m.radius_sm)
        .text_color(if !enabled {
            t.muted_fg_rgba()
        } else if primary {
            IslandTokens::rgba(t.accent)
        } else {
            t.fg_rgba()
        })
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(highlight()))
                .on_click(move |_, window, cx| action(window, cx))
        })
        .child(text(TextRole::Label).child(label))
}

fn tool_body_clamp(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn image_extensions_become_base64_attachments() {
        let dir = tempfile_dir();
        let path = dir.join("a.png");
        std::fs::write(&path, b"hi").unwrap();
        let (attachments, errors) = classify_paths(&[path]);
        assert!(errors.is_empty());
        assert!(matches!(
            &attachments[0].kind,
            AttachmentKind::Image { mime_type, data, .. }
            if mime_type == "image/png" && data.as_str() == base64_of(b"hi")
        ));
    }

    #[test]
    fn unreadable_images_surface_errors_and_other_paths_are_files() {
        let missing = missing_path();
        let (attachments, errors) = classify_paths(&[missing, PathBuf::from("/tmp/x.rs")]);
        assert_eq!(errors.len(), 1);
        assert_eq!(attachments.len(), 1);
        assert!(matches!(&attachments[0].kind, AttachmentKind::File { .. }));
    }

    #[test]
    fn submission_blocks_order_text_mentions_images() {
        let attachments = vec![
            Attachment {
                id: "1".into(),
                kind: AttachmentKind::File {
                    path: "/tmp/x.rs".into(),
                },
            },
            Attachment {
                id: "2".into(),
                kind: AttachmentKind::Image {
                    path: "/tmp/a.png".into(),
                    data: "AA==".into(),
                    mime_type: "image/png".into(),
                },
            },
        ];
        let content = build_submission("look", &attachments).unwrap();
        match content {
            MessageContent::Blocks(blocks) => {
                assert_eq!(blocks.len(), 3);
                assert!(matches!(&blocks[0], ContentBlock::Text { text } if text == "look"));
                assert!(
                    matches!(&blocks[1], ContentBlock::Text { text } if text.contains("@/tmp/x.rs"))
                );
                assert!(
                    matches!(&blocks[2], ContentBlock::Image { mime_type, .. } if mime_type == "image/png")
                );
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn no_attachments_keeps_plain_text_path() {
        assert!(build_submission("hi", &[]).is_none());
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
        assert_eq!(format_token_count(400), "400");
    }

    #[test]
    fn two_row_identity_tracks_the_new_chrome() {
        assert_eq!(
            footprint_for_text("one"),
            2.0 * LINE_HEIGHT + INPUT_PADDING + 78.0 + 20.0
        );
    }

    #[test]
    fn footprint_grows_then_clamps() {
        let short = footprint_for_text("one");
        let many = footprint_for_text(&["x"; 20].join("\n"));
        let eight = footprint_for_text(&["x"; MAX_ROWS].join("\n"));
        assert_eq!(many, eight);
        assert!(many > short);
    }

    fn base64_of(bytes: &[u8]) -> String {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("piko-composer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn missing_path() -> PathBuf {
        std::env::temp_dir().join("piko-missing-image.png")
    }
}
