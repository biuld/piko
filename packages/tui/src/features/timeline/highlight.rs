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

use crate::theme::Theme;

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
