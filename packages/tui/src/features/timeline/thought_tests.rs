use super::component_lines_at;
use crate::features::timeline::{
    ComponentId, ThoughtComponent, ThoughtKey, ThoughtPhase, TimelineComponent,
};
use crate::theme::Theme;
use std::time::{Duration, Instant};

#[test]
fn thought_summary_is_one_row_and_uses_its_own_spinner() {
    let theme = Theme::dark();
    let start = Instant::now();
    let streaming = TimelineComponent::Thought(ThoughtComponent {
        id: ComponentId::Thought(ThoughtKey {
            message_id: "m".into(),
            segment_index: 0,
        }),
        key: ThoughtKey {
            message_id: "m".into(),
            segment_index: 0,
        },
        text: "considering".into(),
        phase: ThoughtPhase::Streaming { observed_at: start },
    });
    let first = component_lines_at(
        &streaming,
        true,
        false,
        &theme,
        24,
        0,
        start + Duration::from_millis(2400),
    );
    let second = component_lines_at(
        &streaming,
        true,
        false,
        &theme,
        24,
        1,
        start + Duration::from_millis(2400),
    );
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    let first_text: String = first[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    let second_text: String = second[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(first_text.contains("◐ thinking... (2.4s)"));
    assert!(second_text.contains("◓ thinking... (2.4s)"));
    assert!(first_text.starts_with(" ◐"));
    let default_style = first[0].spans[0].style;
    assert_eq!(default_style.bg, None);
    assert!(
        default_style
            .add_modifier
            .contains(ratatui::style::Modifier::ITALIC)
    );
    assert!(
        !default_style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD)
    );

    let hovered = component_lines_at(
        &streaming,
        true,
        true,
        &theme,
        24,
        0,
        start + Duration::from_millis(2400),
    );
    let hovered_style = hovered[0].spans[0].style;
    assert_eq!(hovered_style.fg, Some(theme.accent));
    assert_eq!(hovered_style.bg, None);
    assert!(
        hovered_style
            .add_modifier
            .contains(ratatui::style::Modifier::ITALIC)
    );
    assert!(
        hovered_style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD)
    );

    let completed = TimelineComponent::Thought(ThoughtComponent {
        phase: ThoughtPhase::Completed {
            duration_ms: Some(2400),
        },
        ..match streaming {
            TimelineComponent::Thought(thought) => thought,
            _ => unreachable!(),
        }
    });
    let completed_lines = component_lines_at(&completed, true, false, &theme, 24, 0, start);
    assert_eq!(completed_lines.len(), 1);
    let completed_text: String = completed_lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(completed_text.contains("✓ thought in 2.4s"));
    assert!(component_lines_at(&completed, false, false, &theme, 24, 0, start).is_empty());
}
