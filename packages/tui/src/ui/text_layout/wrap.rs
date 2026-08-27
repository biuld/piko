//! Run-based terminal text wrapping.

use std::ops::Range;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::model::{
    Breakability, SourceBoundary, TextLayout, TextRun, VisualFragment, VisualLine, VisualPosition,
};
use crate::terminal::text::TerminalTextPolicy;

/// Prepare runs into visual rows using terminal display columns.
pub fn wrap_runs<P: Clone>(
    runs: impl IntoIterator<Item = TextRun<P>>,
    max_cols: usize,
) -> TextLayout<P> {
    GenericBuilder::new(TerminalTextPolicy, max_cols).build(runs)
}

/// Prepare plain source text with byte ranges covering the original string.
#[allow(dead_code)] // convenience API for source-aware consumers
pub fn wrap_source(text: &str, max_cols: usize) -> TextLayout<()> {
    wrap_runs(
        [TextRun::new(text, (), Breakability::Grapheme).with_source(0..text.len())],
        max_cols,
    )
}

/// Adapt ratatui spans to the shared run kernel.
pub fn wrap_spans(spans: Vec<Span<'static>>, max_cols: usize) -> TextLayout<Style> {
    wrap_runs(
        spans.into_iter().map(|span| {
            TextRun::new(
                span.content.into_owned(),
                span.style,
                Breakability::Grapheme,
            )
        }),
        max_cols,
    )
}

/// Convert a prepared styled layout back into ratatui lines.
pub fn to_lines(layout: &TextLayout<Style>) -> Vec<Line<'static>> {
    layout
        .lines
        .iter()
        .map(|line| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for fragment in &line.fragments {
                if let Some(previous) = spans.last_mut()
                    && previous.style == fragment.payload
                {
                    let text = format!("{}{}", previous.content, fragment.text);
                    *previous = Span::styled(text, fragment.payload);
                } else {
                    spans.push(Span::styled(fragment.text.clone(), fragment.payload));
                }
            }
            Line::from(spans)
        })
        .collect()
}

struct GenericBuilder<P> {
    policy: TerminalTextPolicy,
    max_cols: usize,
    lines: Vec<VisualLine<P>>,
    current: VisualLine<P>,
    boundaries: Vec<SourceBoundary>,
    source_len: usize,
    trailing_hard_line: bool,
}

impl<P: Clone> GenericBuilder<P> {
    fn new(policy: TerminalTextPolicy, max_cols: usize) -> Self {
        Self {
            policy,
            max_cols: max_cols.max(1),
            lines: Vec::new(),
            current: VisualLine::new(false),
            boundaries: Vec::new(),
            source_len: 0,
            trailing_hard_line: false,
        }
    }

    fn build(mut self, runs: impl IntoIterator<Item = TextRun<P>>) -> TextLayout<P> {
        let mut any_run = false;
        for run in runs {
            any_run = true;
            if let Some(source) = &run.source {
                self.source_len = self.source_len.max(source.end);
            }
            self.push_run(&run);
        }
        if !any_run || !self.current.is_empty() || self.lines.is_empty() || self.trailing_hard_line
        {
            self.finish_line(false);
        }
        TextLayout::from_parts(
            self.lines,
            self.max_cols.min(usize::from(u16::MAX)) as u16,
            self.boundaries,
            self.source_len,
        )
    }

    fn push_run(&mut self, run: &TextRun<P>) {
        match run.breakability {
            Breakability::Atomic => self.push_atomic(run),
            Breakability::Grapheme => self.push_graphemes(run, false),
            Breakability::HardBreak => self.push_graphemes(run, true),
        }
    }

    fn push_graphemes(&mut self, run: &TextRun<P>, force_break: bool) {
        let mut segment_start = 0usize;
        for (offset, segment) in run.text.split('\n').enumerate() {
            if offset > 0 {
                let newline_at = segment_start.saturating_sub(1);
                let position = self.current_position();
                self.record_source(run, newline_at, 1, position);
                self.finish_line(true);
                let position = self.current_position();
                self.record_source(run, newline_at.saturating_add(1), 0, position);
                self.trailing_hard_line = true;
            }
            for (relative, grapheme) in self.policy.grapheme_indices(segment) {
                let source_offset = segment_start.saturating_add(relative);
                let width = self.policy.width(grapheme);
                if self.current.width > 0
                    && width > 0
                    && usize::from(self.current.width).saturating_add(width) > self.max_cols
                {
                    self.finish_line(false);
                }
                let start = self.current_position();
                let source_range = self.source_range(run, source_offset, grapheme.len());
                self.record_source(run, source_offset, grapheme.len(), start);
                self.push_fragment(grapheme, run.payload.clone(), width, source_range);
                let position = self.current_position();
                self.record_source(
                    run,
                    source_offset.saturating_add(grapheme.len()),
                    0,
                    position,
                );
            }
            segment_start = segment_start
                .saturating_add(segment.len())
                .saturating_add(1);
        }
        if force_break {
            self.finish_line(true);
        }
    }

    fn push_atomic(&mut self, run: &TextRun<P>) {
        let mut segment_start = 0usize;
        let segments: Vec<&str> = run.text.split('\n').collect();
        for (index, segment) in segments.iter().enumerate() {
            let width = self.policy.width(segment);
            if self.current.width > 0
                && width > 0
                && usize::from(self.current.width).saturating_add(width) > self.max_cols
            {
                self.finish_line(false);
            }
            if !segment.is_empty() {
                let start = self.current_position();
                let source_range = self.source_range(run, segment_start, segment.len());
                self.record_source(run, segment_start, segment.len(), start);
                self.push_fragment(segment, run.payload.clone(), width, source_range);
                let position = self.current_position();
                self.record_source(
                    run,
                    segment_start.saturating_add(segment.len()),
                    0,
                    position,
                );
            }
            if index + 1 < segments.len() {
                let position = self.current_position();
                self.record_source(
                    run,
                    segment_start.saturating_add(segment.len()),
                    1,
                    position,
                );
                self.finish_line(true);
                let position = self.current_position();
                self.record_source(
                    run,
                    segment_start
                        .saturating_add(segment.len())
                        .saturating_add(1),
                    0,
                    position,
                );
                self.trailing_hard_line = true;
            }
            segment_start = segment_start
                .saturating_add(segment.len())
                .saturating_add(1);
        }
    }

    fn push_fragment(
        &mut self,
        text: &str,
        payload: P,
        width: usize,
        source: Option<Range<usize>>,
    ) {
        self.trailing_hard_line = false;
        let start = usize::from(self.current.width);
        let start_col = start.min(usize::from(u16::MAX)) as u16;
        let end = start.saturating_add(width).min(usize::from(u16::MAX)) as u16;
        self.current.fragments.push(VisualFragment {
            text: text.to_string(),
            cols: start_col..end,
            payload,
            source,
        });
        self.current.width = start.saturating_add(width).min(usize::from(u16::MAX)) as u16;
    }

    fn current_position(&self) -> VisualPosition {
        VisualPosition::new(self.lines.len(), self.current.width)
    }

    fn finish_line(&mut self, hard_break: bool) {
        self.current.hard_break = hard_break;
        self.lines
            .push(std::mem::replace(&mut self.current, VisualLine::new(false)));
    }

    fn record_source(
        &mut self,
        run: &TextRun<P>,
        offset: usize,
        _length: usize,
        position: VisualPosition,
    ) {
        let Some(source) = &run.source else {
            return;
        };
        let start = source.start.saturating_add(offset).min(source.end);
        self.boundaries.push(SourceBoundary {
            source: start,
            position,
        });
    }

    fn source_range(&self, run: &TextRun<P>, offset: usize, length: usize) -> Option<Range<usize>> {
        let source = run.source.as_ref()?;
        let start = source.start.saturating_add(offset).min(source.end);
        let end = start.saturating_add(length).min(source.end);
        Some(start..end)
    }
}

/// Return the source range carried by a visual fragment.
#[allow(dead_code)] // convenience API for hit/cursor adapters
pub fn fragment_source<P>(fragment: &VisualFragment<P>) -> Option<Range<usize>> {
    fragment.source.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        style::{Modifier, Style},
        text::Span,
    };

    #[test]
    fn wraps_graphemes_and_keeps_hard_empty_lines() {
        let layout = wrap_source("ab\n\n你好", 3);
        assert_eq!(layout.lines.len(), 4);
        let first: String = layout.lines[0]
            .fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect();
        assert_eq!(first, "ab");
        assert!(layout.lines[1].is_empty());
        assert_eq!(layout.lines[2].width, 2);
    }

    #[test]
    fn styled_adapter_preserves_styles_and_wide_graphemes() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let layout = wrap_spans(vec![Span::styled("a你b", bold)], 3);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].fragments[0].text, "a");
        assert_eq!(layout.lines[1].fragments[0].text, "b");
        assert_eq!(layout.lines[0].fragments[1].text, "你");
        assert_eq!(layout.lines[0].fragments[1].payload, bold);
    }

    #[test]
    fn atomic_run_is_whole_and_source_mapping_round_trips_boundaries() {
        let layout = wrap_runs(
            [TextRun::new("ab", (), Breakability::Atomic).with_source(4..6)],
            1,
        );
        assert_eq!(layout.lines[0].fragments[0].text, "ab");
        assert_eq!(layout.visual_position(4), VisualPosition::new(0, 0));
        assert_eq!(
            layout.source_position(0, 1, super::super::model::PositionBias::Before),
            4
        );
        assert_eq!(
            layout.source_position(0, 1, super::super::model::PositionBias::After),
            6
        );
    }

    #[test]
    fn source_mapping_keeps_graphemes_and_wide_glyphs_valid() {
        let text = "e\u{301}你x";
        let layout = wrap_source(text, 2);
        assert_eq!(layout.lines.len(), 3);
        assert_eq!(layout.lines[0].fragments[0].text, "e\u{301}");
        assert_eq!(layout.lines[1].fragments[0].text, "你");
        assert_eq!(layout.visual_position(4), VisualPosition::new(1, 0));
        assert_eq!(
            layout.source_position(1, 1, super::super::model::PositionBias::Before),
            3
        );
        assert_eq!(
            layout.source_position(1, 1, super::super::model::PositionBias::After),
            6
        );
    }

    #[test]
    fn trailing_newline_has_one_empty_visual_row() {
        let layout = wrap_source("a\n", 4);
        assert_eq!(layout.lines.len(), 2);
        assert!(layout.lines[1].is_empty());
        assert_eq!(layout.line_source_range(1), Some(2..2));
    }
}
