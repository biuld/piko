use std::sync::OnceLock;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style as SyntectStyle, Theme as SyntectTheme},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};
use two_face::theme::EmbeddedThemeName;

use crate::{
    theme::Theme,
    ui::line_layout::{pad_spans, paint_cols, wrap_spans},
};

const MAX_HIGHLIGHT_BYTES: usize = 512 * 1024;
const MAX_HIGHLIGHT_LINES: usize = 10_000;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static DARK_THEME: OnceLock<SyntectTheme> = OnceLock::new();
static LIGHT_THEME: OnceLock<SyntectTheme> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

fn syntax_theme(theme: &Theme) -> &'static SyntectTheme {
    if theme.name.to_ascii_lowercase().contains("light") {
        LIGHT_THEME.get_or_init(|| {
            two_face::theme::extra()
                .get(EmbeddedThemeName::CatppuccinLatte)
                .clone()
        })
    } else {
        DARK_THEME.get_or_init(|| {
            two_face::theme::extra()
                .get(EmbeddedThemeName::CatppuccinMocha)
                .clone()
        })
    }
}

fn find_syntax(language: &str) -> Option<&'static SyntaxReference> {
    let normalized = language.to_ascii_lowercase();
    let language = match normalized.as_str() {
        "csharp" | "c-sharp" => "c#",
        "golang" => "go",
        "python3" => "python",
        "shell" => "bash",
        _ => language,
    };
    let syntaxes = syntax_set();

    syntaxes
        .find_syntax_by_token(language)
        .or_else(|| syntaxes.find_syntax_by_name(language))
        .or_else(|| {
            syntaxes
                .syntaxes()
                .iter()
                .find(|syntax| syntax.name.eq_ignore_ascii_case(language))
        })
        .or_else(|| syntaxes.find_syntax_by_extension(language))
}

fn convert_style(style: SyntectStyle) -> Style {
    let foreground = style.foreground;
    let mut converted = Style::default().fg(Color::Rgb(foreground.r, foreground.g, foreground.b));
    if style.font_style.contains(FontStyle::BOLD) {
        converted = converted.add_modifier(Modifier::BOLD);
    }
    converted
}

fn highlighted_lines(code: &str, language: &str, theme: &Theme) -> Option<Vec<Line<'static>>> {
    if code.len() > MAX_HIGHLIGHT_BYTES || code.lines().count() > MAX_HIGHLIGHT_LINES {
        return None;
    }

    let syntax = find_syntax(language)?;
    let mut highlighter = HighlightLines::new(syntax, syntax_theme(theme));
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, syntax_set()).ok()?;
        let spans = ranges
            .into_iter()
            .filter_map(|(style, text)| {
                let text = text.trim_end_matches(['\n', '\r']);
                (!text.is_empty()).then(|| Span::styled(text.to_string(), convert_style(style)))
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(spans));
    }
    Some(lines)
}

fn plain_lines(code: &str) -> Vec<Line<'static>> {
    let mut lines = code
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::default());
    }
    lines
}

pub(super) fn highlight_code_to_lines(
    code: &str,
    language: Option<&str>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    language
        .and_then(|language| highlighted_lines(code, language, theme))
        .unwrap_or_else(|| plain_lines(code))
}

/// Return the extension token used by syntect for a workspace path.
pub(super) fn language_from_path(path: &str) -> Option<&str> {
    std::path::Path::new(path).extension()?.to_str()
}

/// Highlight one independent source row, applying caller-owned foreground and
/// background semantics when syntax is unknown.
pub(super) fn code_line_spans(
    text: &str,
    language: Option<&str>,
    theme: &Theme,
    plain_foreground: Color,
    background: Color,
) -> Vec<Span<'static>> {
    let fallback = Style::default().fg(plain_foreground).bg(background);
    let mut spans = highlight_code_to_lines(text, language, theme)
        .into_iter()
        .next()
        .map(|line| line.spans)
        .unwrap_or_default();
    if spans.is_empty() {
        return vec![Span::styled(String::new(), fallback)];
    }
    for span in &mut spans {
        if span.style.fg.is_none() {
            span.style = span.style.fg(plain_foreground);
        }
        span.style = span.style.bg(background);
    }
    spans
}

/// Paint a source listing shared by Markdown fences and workspace tool bodies.
///
/// Every source row owns a stable line-number gutter. Soft-wrapped continuation
/// rows retain a blank gutter so code never slides underneath the line numbers.
#[allow(clippy::too_many_arguments)]
pub(super) fn code_listing_lines(
    code: &str,
    language: Option<&str>,
    start_line: usize,
    theme: &Theme,
    background: Color,
    plain_foreground: Color,
    gutter_foreground: Color,
    width: u16,
    leading_space: bool,
) -> Vec<Line<'static>> {
    let source = highlight_code_to_lines(code, language, theme);
    let gutter_width = start_line
        .saturating_add(source.len().saturating_sub(1))
        .max(1)
        .to_string()
        .len();
    let gutter_style = Style::default().fg(gutter_foreground).bg(background);
    let body_fallback = Style::default().fg(plain_foreground).bg(background);
    let lead = usize::from(leading_space);
    // Keep a two-cell content inset after the line-number gutter. The gutter
    // uses alignment, color, and whitespace instead of a selectable separator
    // glyph, matching code editors without polluting copied source listings.
    let prefix_width = lead + gutter_width + 2;
    let body_width = usize::from(width).saturating_sub(prefix_width).max(1);
    let mut output = Vec::new();

    for (index, line) in source.into_iter().enumerate() {
        let mut spans = line.spans;
        if spans.is_empty() {
            spans.push(Span::styled(String::new(), body_fallback));
        } else {
            for span in &mut spans {
                if span.style.fg.is_none() {
                    span.style = span.style.fg(plain_foreground);
                }
                span.style = span.style.bg(background);
            }
        }
        let wrapped = wrap_spans(spans, body_width);
        let wrapped = if wrapped.is_empty() {
            vec![Line::from(Span::styled(String::new(), body_fallback))]
        } else {
            wrapped
        };

        for (visual_index, row) in wrapped.into_iter().enumerate() {
            let number = if visual_index == 0 {
                start_line.saturating_add(index).to_string()
            } else {
                String::new()
            };
            let mut row_spans = Vec::new();
            if leading_space {
                row_spans.push(Span::styled(" ", gutter_style));
            }
            row_spans.push(Span::styled(
                format!("{number:>gutter_width$}"),
                gutter_style,
            ));
            row_spans.push(Span::styled("  ", gutter_style));
            row_spans.extend(row.spans);
            let used = row_spans
                .iter()
                .map(|span| paint_cols(span.content.as_ref()))
                .sum::<usize>();
            if used > usize::from(width) {
                // `body_width` is at least one for ultra-narrow terminals; the
                // final painter remains bounded even when chrome alone is wider.
                output.push(Line::from(row_spans));
            } else {
                output.push(pad_spans(row_spans, body_fallback, width));
            }
        }
    }
    output
}
