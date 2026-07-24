//! Inline semantic flattening into GPUI styled text runs.

use gpui::{
    FontStyle, FontWeight, SharedString, StrikethroughStyle, StyledText, TextRun, TextStyle,
    UnderlineStyle, px,
};

use crate::theme::{ChromeTokens, tokens};

use super::super::model::{InlineStyle, MarkdownInline};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct InlineMarks {
    strong: bool,
    emphasis: bool,
    strikethrough: bool,
    code: bool,
    link: bool,
}

pub(crate) fn styled_inline(content: &[MarkdownInline], base_strong: bool) -> StyledText {
    let mut flattened = Flattened::default();
    flattened.push_all(
        content,
        InlineMarks {
            strong: base_strong,
            ..InlineMarks::default()
        },
    );
    flattened.into_styled_text(tokens())
}

#[derive(Default)]
struct Flattened {
    text: String,
    runs: Vec<(usize, InlineMarks)>,
}

impl Flattened {
    fn push_all(&mut self, content: &[MarkdownInline], marks: InlineMarks) {
        for inline in content {
            self.push(inline, marks);
        }
    }

    fn push(&mut self, inline: &MarkdownInline, marks: InlineMarks) {
        match inline {
            MarkdownInline::Text(text) => self.push_text(text, marks),
            MarkdownInline::Styled { kind, children } => {
                let mut nested = marks;
                match kind {
                    InlineStyle::Emphasis => nested.emphasis = true,
                    InlineStyle::Strong => nested.strong = true,
                    InlineStyle::Strikethrough => nested.strikethrough = true,
                }
                self.push_all(children, nested);
            }
            MarkdownInline::Code(code) => {
                self.push_text(
                    code,
                    InlineMarks {
                        code: true,
                        ..marks
                    },
                );
            }
            MarkdownInline::Link { children, .. } => {
                self.push_all(
                    children,
                    InlineMarks {
                        link: true,
                        ..marks
                    },
                );
            }
            MarkdownInline::ImageAlt(children) => self.push_all(children, marks),
            MarkdownInline::SoftBreak => self.push_text(" ", marks),
            MarkdownInline::HardBreak => self.push_text("\n", marks),
        }
    }

    fn push_text(&mut self, text: &str, marks: InlineMarks) {
        if text.is_empty() {
            return;
        }
        self.text.push_str(text);
        if let Some((len, previous)) = self.runs.last_mut()
            && *previous == marks
        {
            *len += text.len();
        } else {
            self.runs.push((text.len(), marks));
        }
    }

    fn into_styled_text(self, palette: ChromeTokens) -> StyledText {
        let runs = self
            .runs
            .into_iter()
            .map(|(len, marks)| text_run(len, marks, palette))
            .collect();
        StyledText::new(SharedString::from(self.text)).with_runs(runs)
    }
}

fn text_run(len: usize, marks: InlineMarks, palette: ChromeTokens) -> TextRun {
    let mut style = TextStyle {
        color: ChromeTokens::hsla(palette.fg),
        ..TextStyle::default()
    };
    if marks.strong {
        style.font_weight = FontWeight::SEMIBOLD;
    }
    if marks.emphasis {
        style.font_style = FontStyle::Italic;
    }
    if marks.strikethrough {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(ChromeTokens::hsla(palette.muted_fg)),
        });
    }
    if marks.code {
        style.font_family = "monospace".into();
        style.background_color = Some(ChromeTokens::hsla(palette.elevated));
    }
    if marks.link {
        style.color = ChromeTokens::hsla(palette.accent);
        style.underline = Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(ChromeTokens::hsla(palette.accent)),
            wavy: false,
        });
    }
    style.to_run(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattening_preserves_breaks_and_image_alt_text() {
        let mut flattened = Flattened::default();
        flattened.push_all(
            &[
                MarkdownInline::Text("before".into()),
                MarkdownInline::SoftBreak,
                MarkdownInline::ImageAlt(vec![MarkdownInline::Text("diagram".into())]),
                MarkdownInline::HardBreak,
                MarkdownInline::Code("code".into()),
            ],
            InlineMarks::default(),
        );
        assert_eq!(flattened.text, "before diagram\ncode");
        assert_eq!(flattened.runs.iter().map(|run| run.0).sum::<usize>(), 19);
    }
}
