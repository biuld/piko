use ratatui::style::Color;

use super::parse_markdown;
use crate::theme::Theme;

fn rendered_text(lines: &[ratatui::text::Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn fenced_code_block_renders_without_pseudo_border() {
    let lines = parse_markdown("```\nfirst\nsecond\n```\n", &Theme::dark());

    assert_eq!(rendered_text(&lines), vec!["first", "second"]);
    assert!(lines.iter().all(|line| {
        line.spans.iter().all(|span| {
            !span.content.contains("Code Block")
                && !span.content.contains('│')
                && !span.content.contains('┌')
                && !span.content.contains('└')
        })
    }));
}

#[test]
fn fenced_code_block_is_separated_from_preceding_prose() {
    let lines = parse_markdown("Before\n\n```\ncode\n```\n", &Theme::dark());

    assert_eq!(rendered_text(&lines), vec!["Before", "", "code"]);
}

#[test]
fn known_fenced_language_uses_syntax_highlighting() {
    let lines = parse_markdown(
        "```rust,no_run\nfn main() { let value = \"text\"; }\n```\n",
        &Theme::dark(),
    );
    let colors = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .filter_map(|span| span.style.fg)
        .fold(Vec::<Color>::new(), |mut colors, color| {
            if !colors.contains(&color) {
                colors.push(color);
            }
            colors
        });

    assert_eq!(
        rendered_text(&lines),
        vec!["fn main() { let value = \"text\"; }"]
    );
    assert!(colors.len() > 1, "expected multiple syntax colors");
}

#[test]
fn unknown_fenced_language_falls_back_to_plain_text() {
    let lines = parse_markdown("```not-a-language\nhello world\n```\n", &Theme::dark());

    assert_eq!(rendered_text(&lines), vec!["hello world"]);
    assert!(
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .all(|span| span.style == Default::default())
    );
}
