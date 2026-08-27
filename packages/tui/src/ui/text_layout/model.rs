//! Data model for column-aware terminal text layouts.

use std::ops::Range;

/// How a text run may be divided between visual rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Breakability {
    /// Wrap at grapheme-cluster boundaries.
    Grapheme,
    /// Keep the complete run on one visual row; paint may clip an over-wide
    /// run, but the run is never split at an invalid source boundary.
    Atomic,
    /// End the current visual row after this run.
    HardBreak,
}

/// Styled or semantically annotated input to the wrap kernel.
#[derive(Clone, Debug)]
pub struct TextRun<P> {
    pub text: String,
    pub payload: P,
    pub source: Option<Range<usize>>,
    pub breakability: Breakability,
}

impl<P> TextRun<P> {
    pub fn new(text: impl Into<String>, payload: P, breakability: Breakability) -> Self {
        Self {
            text: text.into(),
            payload,
            source: None,
            breakability,
        }
    }

    pub fn with_source(mut self, source: Range<usize>) -> Self {
        if source.start <= source.end {
            let end = source.start.saturating_add(self.text.len()).min(source.end);
            self.source = Some(source.start..end);
        } else {
            self.source = None;
        }
        self
    }
}

/// One styled/owned fragment in a visual line.  `cols` is relative to the
/// line's left edge and is half-open.
#[derive(Clone, Debug)]
pub struct VisualFragment<P> {
    pub text: String,
    #[allow(dead_code)] // retained for column-aware hit/paint adapters
    pub cols: Range<u16>,
    pub payload: P,
    pub source: Option<Range<usize>>,
}

/// One terminal row after wrapping.
#[derive(Clone, Debug)]
pub struct VisualLine<P> {
    pub fragments: Vec<VisualFragment<P>>,
    pub width: u16,
    pub hard_break: bool,
}

impl<P> VisualLine<P> {
    pub fn new(hard_break: bool) -> Self {
        Self {
            fragments: Vec::new(),
            width: 0,
            hard_break,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }
}

/// A visual row/column coordinate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct VisualPosition {
    pub row: usize,
    pub col: u16,
}

impl VisualPosition {
    pub const fn new(row: usize, col: u16) -> Self {
        Self { row, col }
    }
}

/// Which valid source boundary to choose when a visual cell lies between
/// source positions (notably inside a wide or atomic fragment).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PositionBias {
    #[default]
    Before,
    #[allow(dead_code)] // used by pointer adapters that prefer the trailing boundary
    After,
    Nearest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceBoundary {
    pub source: usize,
    pub position: VisualPosition,
}

/// Prepared visual text.  Paint and pointer/cursor mapping can consume this
/// same snapshot without repeating wrapping arithmetic.
#[derive(Clone, Debug)]
pub struct TextLayout<P> {
    pub lines: Vec<VisualLine<P>>,
    #[allow(dead_code)] // retained as the prepared width contract for consumers
    pub width: u16,
    pub(crate) source_boundaries: Vec<SourceBoundary>,
    pub(crate) source_len: usize,
}

impl<P> TextLayout<P> {
    pub(crate) fn from_parts(
        lines: Vec<VisualLine<P>>,
        width: u16,
        mut source_boundaries: Vec<SourceBoundary>,
        source_len: usize,
    ) -> Self {
        source_boundaries.sort_by_key(|boundary| (boundary.source, boundary.position));
        Self {
            lines,
            width,
            source_boundaries,
            source_len,
        }
    }

    pub fn row_count(&self) -> usize {
        self.lines.len()
    }

    #[allow(dead_code)] // public source-coordinate contract for editor integrations
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// Source range represented by one visual row.  Empty hard lines use the
    /// valid boundary at the start of that row.
    pub fn line_source_range(&self, row: usize) -> Option<Range<usize>> {
        let line = self.lines.get(row)?;
        let ranges: Vec<&Range<usize>> = line
            .fragments
            .iter()
            .filter_map(|fragment| fragment.source.as_ref())
            .collect();
        if let (Some(first), Some(last)) = (ranges.first(), ranges.last()) {
            return Some(first.start..last.end);
        }
        let boundary = self
            .source_boundaries
            .iter()
            .filter(|boundary| boundary.position.row == row)
            .min_by_key(|boundary| boundary.position.col);
        boundary
            .map(|boundary| boundary.source..boundary.source)
            .or_else(|| (self.source_len == 0).then_some(0..0))
    }

    /// Map a source byte position to the nearest preceding valid visual
    /// boundary.  Source positions inside a grapheme/atomic run therefore
    /// resolve to the run's start; positions at its end resolve after it.
    pub fn visual_position(&self, source: usize) -> VisualPosition {
        let Some(first) = self.source_boundaries.first() else {
            return VisualPosition::default();
        };
        let source = source.min(self.source_len);
        let mut result = first.position;
        for boundary in &self.source_boundaries {
            if boundary.source > source {
                break;
            }
            result = boundary.position;
        }
        result
    }

    /// Map a visual row/column to a valid source boundary.
    pub fn source_position(&self, row: usize, col: u16, bias: PositionBias) -> usize {
        let Some(first) = self.source_boundaries.first() else {
            return 0;
        };
        let row = row.min(self.lines.len().saturating_sub(1));
        let target = VisualPosition::new(row, col);
        let before = self
            .source_boundaries
            .iter()
            .filter(|boundary| boundary.position <= target)
            .max_by_key(|boundary| boundary.position);
        let after = self
            .source_boundaries
            .iter()
            .filter(|boundary| boundary.position >= target)
            .min_by_key(|boundary| boundary.position);
        let selected = match bias {
            PositionBias::Before => before.or(after),
            PositionBias::After => after.or(before),
            PositionBias::Nearest => match (before, after) {
                (Some(before), Some(after)) => {
                    let before_distance = target.col.saturating_sub(before.position.col);
                    let after_distance = after.position.col.saturating_sub(target.col);
                    (after_distance <= before_distance)
                        .then_some(after)
                        .or(Some(before))
                }
                (Some(before), None) => Some(before),
                (None, Some(after)) => Some(after),
                (None, None) => None,
            },
        };
        selected.unwrap_or(first).source.min(self.source_len)
    }
}
