//! Centered, read-only inspector for one Timeline thought segment.

use std::{cell::Cell, time::Instant};

use piko_tui_layout::{Component, SurfacePanel, ViewportMetrics, ViewportState};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::{
    app::HitId,
    features::timeline::{ThoughtKey, ThoughtPhase, Timeline},
    navigation::SurfaceId,
    theme::Theme,
    ui::{
        components::{
            pane::{PaneFooter, PaneSpec, render_pane},
            scroll_view::paint_scroll_view,
        },
        interaction::{ComponentHit, PointerComponent, PointerGesture},
        text_layout::{Breakability, TextRun, wrap_runs},
    },
};

const REVEAL_BATCH: usize = 8;
const CURSOR_GLYPH: &str = "▌";

pub struct ThoughtInspectorCtx<'a> {
    pub timeline: &'a Timeline,
    pub theme: &'a Theme,
    pub now: Instant,
}

/// Ephemeral surface state. The thought text is deliberately absent: render
/// resolves it from the selected Timeline by semantic key on every frame.
pub struct ThoughtInspectorState {
    key: ThoughtKey,
    viewport: Cell<ViewportState>,
    reveal_cursor: usize,
    last_reveal_tick: Option<Instant>,
}

impl ThoughtInspectorState {
    pub fn new() -> Self {
        Self {
            key: ThoughtKey {
                message_id: String::new(),
                segment_index: 0,
            },
            viewport: Cell::new(ViewportState::default()),
            reveal_cursor: 0,
            last_reveal_tick: None,
        }
    }

    pub fn key(&self) -> &ThoughtKey {
        &self.key
    }

    pub fn open(&mut self, thought: &crate::features::timeline::ThoughtComponent, now: Instant) {
        self.key = thought.key.clone();
        self.reveal_cursor = thought.text.graphemes(true).count();
        self.last_reveal_tick =
            matches!(thought.phase, ThoughtPhase::Streaming { .. }).then_some(now);
        self.viewport.set(ViewportState::default());
    }

    pub fn scroll_up(&mut self, amount: usize) {
        let mut viewport = self.viewport.get();
        viewport.scroll_by(-(amount.min(isize::MAX as usize) as isize));
        self.viewport.set(viewport);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let mut viewport = self.viewport.get();
        viewport.scroll_by(amount.min(isize::MAX as usize) as isize);
        self.viewport.set(viewport);
    }

    /// Reveal newly arrived live content in bounded grapheme batches. Returns
    /// false when the key is no longer present in the canonical projection.
    #[allow(dead_code)]
    pub fn advance_reveal(&mut self, timeline: &Timeline, now: Instant) -> bool {
        let Some(thought) = timeline.thought(&self.key) else {
            return false;
        };
        self.advance_reveal_to(&thought, now);
        true
    }

    pub fn advance_reveal_to(
        &mut self,
        thought: &crate::features::timeline::ThoughtComponent,
        now: Instant,
    ) {
        let target = thought.text.graphemes(true).count();
        if matches!(thought.phase, ThoughtPhase::Completed { .. }) {
            self.reveal_cursor = target;
            self.last_reveal_tick = None;
            return;
        }
        self.reveal_cursor = self.reveal_cursor.min(target);
        if target > self.reveal_cursor {
            self.reveal_cursor = self.reveal_cursor.saturating_add(REVEAL_BATCH).min(target);
            self.last_reveal_tick = Some(now);
        }
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        timeline: &Timeline,
        theme: &Theme,
        now: Instant,
    ) {
        let Some(thought) = timeline.thought(&self.key) else {
            return;
        };
        let duration = super::timeline::phase_duration_ms(thought.phase, now)
            .map(super::timeline::format_duration_ms);
        let title = match duration {
            Some(duration) => format!("Thought · {duration}"),
            None => "Thought".to_string(),
        };
        let spec = PaneSpec::new(&title)
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true);
        let Some(areas) = render_pane(frame, area, &spec, theme) else {
            return;
        };

        let total = thought.text.graphemes(true).count();
        let visible_count = if matches!(thought.phase, ThoughtPhase::Completed { .. }) {
            total
        } else {
            self.reveal_cursor.min(total)
        };
        let visible = thought
            .text
            .graphemes(true)
            .take(visible_count)
            .collect::<String>();
        let cursor =
            matches!(thought.phase, ThoughtPhase::Streaming { .. }) && visible_count < total;
        let mut runs = Vec::new();
        let text_style = Style::default()
            .fg(theme.thinking_text)
            .add_modifier(Modifier::ITALIC);
        for (index, line) in visible.split('\n').enumerate() {
            if index > 0 {
                runs.push(TextRun::new(
                    String::new(),
                    text_style,
                    Breakability::HardBreak,
                ));
            }
            runs.push(TextRun::new(line, text_style, Breakability::Grapheme));
        }
        if cursor {
            runs.push(TextRun::new(
                CURSOR_GLYPH,
                Style::default().fg(theme.accent),
                Breakability::Atomic,
            ));
        }
        if runs.is_empty() {
            runs.push(TextRun::new("", text_style, Breakability::Grapheme));
        }
        let layout = wrap_runs(
            runs,
            usize::from(areas.content.width.saturating_sub(1).max(1)),
        );
        let mut viewport = self.viewport.get();
        viewport.update_metrics(ViewportMetrics::new(
            layout.row_count(),
            usize::from(areas.content.height),
        ));
        let plan = viewport.prepare(areas.content, 1);
        self.viewport.set(viewport);
        paint_scroll_view(frame, &layout, &plan, theme);
        if let Some(footer) = spec.footer_rect(area) {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Esc close · scroll ↕",
                    Style::default().fg(theme.dim),
                ))),
                footer,
            );
        }
    }

    fn pane_spec() -> PaneSpec<'static> {
        PaneSpec::new("Thought")
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true)
    }
}

impl Default for ThoughtInspectorState {
    fn default() -> Self {
        Self::new()
    }
}

impl Component<HitId, ThoughtInspectorCtx<'_>> for ThoughtInspectorState {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &ThoughtInspectorCtx<'_>) {
        self.render(frame, area, ctx.timeline, ctx.theme, ctx.now);
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        Self::pane_spec()
            .content_rect(area)
            .map(|content| {
                let content = Rect::new(
                    content.x,
                    content.y,
                    content.width.saturating_sub(1),
                    content.height,
                );
                (content.width > 0 && content.height > 0)
                    .then_some((content, HitId::Content))
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl SurfacePanel<SurfaceId, HitId, ThoughtInspectorCtx<'_>> for ThoughtInspectorState {
    fn region(&self) -> SurfaceId {
        SurfaceId::ThoughtInspector
    }
}

impl PointerComponent<HitId> for ThoughtInspectorState {
    fn pointer_event(
        &mut self,
        hit: ComponentHit<HitId>,
        gesture: PointerGesture,
    ) -> Vec<crate::app::command::Action> {
        if hit.element != Some(HitId::Content) {
            return Vec::new();
        }
        match gesture {
            PointerGesture::ScrollUp => self.scroll_up(3),
            PointerGesture::ScrollDown => self.scroll_down(3),
            PointerGesture::Activate => {}
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::timeline::{ComponentId, ThoughtComponent, TimelineComponent};

    fn thought(text: &str, phase: ThoughtPhase) -> ThoughtComponent {
        let key = ThoughtKey {
            message_id: "message-1".into(),
            segment_index: 0,
        };
        ThoughtComponent {
            id: ComponentId::Thought(key.clone()),
            key,
            text: text.into(),
            phase,
        }
    }

    #[test]
    fn opening_primes_received_content_and_reveals_new_graphemes() {
        let start = Instant::now();
        let initial = thought("already 🙂", ThoughtPhase::Streaming { observed_at: start });
        let key = initial.key.clone();
        let mut timeline = Timeline::new();
        timeline
            .components
            .push_back(TimelineComponent::Thought(initial.clone()));
        let mut state = ThoughtInspectorState::new();
        state.open(&initial, start);
        assert_eq!(
            state.reveal_cursor,
            initial.text.graphemes(true).count(),
            "opening must show already received content"
        );

        let expanded = thought(
            "already 🙂 plus a newly arrived tail",
            ThoughtPhase::Streaming { observed_at: start },
        );
        timeline.components[0] = TimelineComponent::Thought(expanded.clone());
        assert!(state.advance_reveal(&timeline, start + std::time::Duration::from_millis(16)));
        let target = expanded.text.graphemes(true).count();
        assert!(state.reveal_cursor <= target);
        assert!(state.reveal_cursor > initial.text.graphemes(true).count());
        assert_eq!(state.key, key);
        assert!(state.last_reveal_tick.is_some());
    }

    #[test]
    fn completion_reveals_buffered_tail_and_missing_key_is_reported() {
        let start = Instant::now();
        let live = thought(
            &"x".repeat(40),
            ThoughtPhase::Streaming { observed_at: start },
        );
        let mut timeline = Timeline::new();
        timeline
            .components
            .push_back(TimelineComponent::Thought(live.clone()));
        let mut state = ThoughtInspectorState::new();
        state.open(&live, start);

        let longer = thought(
            &"x".repeat(80),
            ThoughtPhase::Streaming { observed_at: start },
        );
        timeline.components[0] = TimelineComponent::Thought(longer.clone());
        assert!(state.advance_reveal(&timeline, start + std::time::Duration::from_millis(16)));
        assert_eq!(
            state.reveal_cursor, 48,
            "reveal advances in a bounded batch"
        );

        let completed = thought(
            &"x".repeat(80),
            ThoughtPhase::Completed {
                duration_ms: Some(2400),
            },
        );
        timeline.components[0] = TimelineComponent::Thought(completed.clone());
        assert!(state.advance_reveal(&timeline, start + std::time::Duration::from_millis(32)));
        assert_eq!(state.reveal_cursor, 80);
        assert_eq!(state.last_reveal_tick, None);

        timeline.components.clear();
        assert!(!state.advance_reveal(&timeline, start));
    }
}
