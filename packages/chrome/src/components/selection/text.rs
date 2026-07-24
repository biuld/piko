//! Styled text leaf that maps a row selection onto its own glyph range.

use std::ops::Range;

use gpui::*;

use super::SelectionState;
use crate::theme::{ChromeTokens, tokens};

pub struct SelectableText {
    id: ElementId,
    fragment_id: SharedString,
    value: SharedString,
    text: StyledText,
    state: Entity<SelectionState>,
}

impl SelectableText {
    pub fn new(
        id: impl Into<ElementId>,
        fragment_id: impl Into<SharedString>,
        value: impl Into<SharedString>,
        text: StyledText,
        state: Entity<SelectionState>,
    ) -> Self {
        Self {
            id: id.into(),
            fragment_id: fragment_id.into(),
            value: value.into(),
            text,
            state,
        }
    }
}

impl IntoElement for SelectableText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SelectableText {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.text.request_layout(None, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.text
            .prepaint(None, inspector_id, bounds, state, window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.text.layout().clone();
        let selection = self.state.read(cx);
        let selected = selection.fixed_range(&self.fragment_id).or_else(|| {
            selection.endpoints().and_then(|endpoints| {
                selected_range(&layout, bounds, self.value.as_ref(), endpoints)
            })
        });
        let highlights = selected
            .as_ref()
            .map(|range| selection_bounds(range, &layout, bounds))
            .unwrap_or_default();
        for highlight in &highlights {
            paint_selection(*highlight, window);
        }
        self.text
            .paint(None, inspector_id, bounds, &mut (), &mut (), window, cx);
        let fragment_id = self.fragment_id.clone();
        let text = self.value.clone();
        self.state.update(cx, |state, _| {
            state.record(fragment_id, bounds, text, selected, layout, highlights)
        });
    }
}

fn selected_range(
    layout: &TextLayout,
    bounds: Bounds<Pixels>,
    text: &str,
    (mut start, mut end): (Point<Pixels>, Point<Pixels>),
) -> Option<Range<usize>> {
    if (end.y, end.x) < (start.y, start.x) {
        std::mem::swap(&mut start, &mut end);
    }
    if end.y < bounds.top() || start.y > bounds.bottom() {
        return None;
    }
    let start = endpoint_index(layout, bounds, text, start);
    let end = endpoint_index(layout, bounds, text, end);
    (start != end).then_some(start.min(end)..start.max(end))
}

fn endpoint_index(
    layout: &TextLayout,
    bounds: Bounds<Pixels>,
    text: &str,
    point: Point<Pixels>,
) -> usize {
    if point.y < bounds.top() {
        return 0;
    }
    if point.y > bounds.bottom() {
        return text.len();
    }
    let index = match layout.index_for_position(point) {
        Ok(index) | Err(index) => index,
    }
    .min(text.len());
    floor_char_boundary(text, index)
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn selection_bounds(
    range: &Range<usize>,
    layout: &TextLayout,
    bounds: Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    let Some(start) = layout.position_for_index(range.start) else {
        return Vec::new();
    };
    let Some(end) = layout.position_for_index(range.end) else {
        return Vec::new();
    };
    let height = layout.line_height();
    let mut result = Vec::with_capacity(3);
    if start.y == end.y {
        result.push(Bounds::from_corners(start, point(end.x, end.y + height)));
        return result;
    }
    result.push(Bounds::from_corners(
        start,
        point(bounds.right(), start.y + height),
    ));
    if end.y > start.y + height {
        result.push(Bounds::from_corners(
            point(bounds.left(), start.y + height),
            point(bounds.right(), end.y),
        ));
    }
    result.push(Bounds::from_corners(
        point(bounds.left(), end.y),
        point(end.x, end.y + height),
    ));
    result
}

fn paint_selection(bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(quad(
        bounds,
        px(0.),
        ChromeTokens::hsla(tokens().accent).opacity(0.28),
        Edges::default(),
        transparent_black(),
        BorderStyle::default(),
    ));
}
