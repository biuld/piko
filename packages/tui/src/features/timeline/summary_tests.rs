use super::component_lines_at;
use crate::features::timeline::{
    ComponentId, SummaryComponent, SummaryKind, SummaryPhase, TimelineComponent,
};
use crate::theme::Theme;
use std::time::{Duration, Instant};

fn compaction(phase: SummaryPhase, text: &str) -> TimelineComponent {
    TimelineComponent::Summary(SummaryComponent {
        id: ComponentId::LiveCompaction,
        kind: SummaryKind::Compaction,
        text: text.into(),
        phase,
        tokens_before: Some(12_000),
        tokens_after: Some(3_000),
        new_context_window: false,
    })
}

fn plain(lines: &[ratatui::text::Line<'_>]) -> Vec<String> {
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
fn compacting_card_uses_spinner_and_pending_background() {
    let theme = Theme::dark();
    let start = Instant::now();
    let running = compaction(SummaryPhase::Running { observed_at: start }, "");
    let first = component_lines_at(
        &running,
        true,
        false,
        &theme,
        40,
        0,
        start + Duration::from_millis(2300),
    );
    let second = component_lines_at(
        &running,
        true,
        false,
        &theme,
        40,
        1,
        start + Duration::from_millis(2300),
    );
    let first_text = plain(&first);
    let second_text = plain(&second);
    assert!(
        first_text
            .iter()
            .any(|line| line.contains("Compacting conversation")),
        "{first_text:?}"
    );
    assert!(
        first_text.iter().any(|line| line.contains("2.3s")),
        "{first_text:?}"
    );
    assert_ne!(first_text, second_text, "spinner frame should change");
    assert_eq!(first[1].spans[0].style.bg, Some(theme.tool_pending_bg));
}

#[test]
fn compacted_card_is_a_status_block_with_token_chip() {
    let theme = Theme::dark();
    let lines = component_lines_at(
        &compaction(SummaryPhase::Completed, "kept recent turns"),
        true,
        false,
        &theme,
        48,
        0,
        Instant::now(),
    );
    let text = plain(&lines);
    assert!(
        text.iter()
            .any(|line| line.contains("Conversation compacted")),
        "{text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.contains("12k") && line.contains("3k")),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line.contains("kept recent turns")),
        "{text:?}"
    );
    assert_eq!(lines[1].spans[0].style.bg, Some(theme.tool_success_bg));
}

#[test]
fn failed_compaction_card_uses_error_tone() {
    let theme = Theme::dark();
    let lines = component_lines_at(
        &compaction(SummaryPhase::Failed, "summarizer failed"),
        true,
        false,
        &theme,
        40,
        0,
        Instant::now(),
    );
    let text = plain(&lines);
    assert!(
        text.iter().any(|line| line.contains("Compaction failed")),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line.contains("summarizer failed")),
        "{text:?}"
    );
    assert_eq!(lines[1].spans[0].style.bg, Some(theme.tool_error_bg));
}

#[test]
fn branch_summary_stays_a_notice_line() {
    let theme = Theme::dark();
    let component = TimelineComponent::Summary(SummaryComponent {
        id: ComponentId::EntryId("branch".into()),
        kind: SummaryKind::Branch,
        text: "branched here".into(),
        phase: SummaryPhase::Completed,
        tokens_before: None,
        tokens_after: None,
        new_context_window: false,
    });
    let lines = component_lines_at(&component, true, false, &theme, 32, 0, Instant::now());
    let text = plain(&lines);
    assert_eq!(lines.len(), 1);
    assert!(
        text[0].contains("branch summary") && text[0].contains("branched here"),
        "{text:?}"
    );
    assert_eq!(lines[0].spans[0].style.bg, None);
}
