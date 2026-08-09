//! Shared one-row dock chrome for notices and contextual key hints.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::theme::Theme;

use super::feedback::hint_style;

/// Render a pre-composed line into a fixed Dock row.
///
/// `Paragraph` is intentionally left unwrapped: a narrow terminal clips the
/// trailing content instead of expanding the row into a second line.
pub fn render(frame: &mut Frame<'_>, area: Rect, line: Line<'_>, background: Option<Color>) {
    let mut paragraph = Paragraph::new(line);
    if let Some(background) = background {
        paragraph = paragraph.style(Style::default().bg(background));
    }
    frame.render_widget(paragraph, area);
}

/// Standard passive keyboard-guidance line.
pub fn hint_line(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(" ⌨ ", hint_style(theme)),
        Span::styled(text.to_string(), hint_style(theme)),
    ])
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    use super::*;

    #[test]
    fn hint_line_has_keyboard_marker() {
        let line = hint_line("Esc close", &Theme::dark());
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, " ⌨ Esc close");
    }

    #[test]
    fn renderer_clips_to_its_single_row() {
        let mut terminal = Terminal::new(TestBackend::new(8, 1)).unwrap();
        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 8, 1),
                    hint_line("Esc close", &Theme::dark()),
                    None,
                )
            })
            .unwrap();
        assert_eq!(terminal.backend().buffer().area.height, 1);
    }
}
