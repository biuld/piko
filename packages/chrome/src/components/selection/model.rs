//! Row-scoped selection state shared by selectable text fragments.

use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use gpui::*;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Default)]
pub struct SelectionGroup(Rc<RefCell<Option<SharedString>>>);

impl SelectionGroup {
    pub fn new() -> Self {
        Self::default()
    }

    fn activate(&self, id: SharedString) {
        *self.0.borrow_mut() = Some(id);
    }

    fn is_active(&self, id: &SharedString) -> bool {
        self.0.borrow().as_ref() == Some(id)
    }
}

pub struct SelectionState {
    focus_handle: FocusHandle,
    id: SharedString,
    group: SelectionGroup,
    anchor: Option<Point<Pixels>>,
    head: Option<Point<Pixels>>,
    selecting: bool,
    fixed: Option<(SharedString, Range<usize>)>,
    generation: u64,
    fragments: HashMap<SharedString, SelectionFragment>,
}

#[derive(Clone)]
pub(crate) struct SelectionFragment {
    pub(crate) generation: u64,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) text: SharedString,
    pub(crate) range: Option<Range<usize>>,
    pub(crate) layout: TextLayout,
    pub(crate) highlights: Vec<Bounds<Pixels>>,
}

impl SelectionState {
    pub fn new(id: impl Into<SharedString>, group: SelectionGroup, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            id: id.into(),
            group,
            anchor: None,
            head: None,
            selecting: false,
            fixed: None,
            generation: 0,
            fragments: HashMap::new(),
        }
    }

    pub(crate) fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub(crate) fn begin_frame(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn start(&mut self, position: Point<Pixels>, click_count: usize) {
        self.group.activate(self.id.clone());
        if click_count >= 2
            && let Some((fragment_id, range)) = self.word_at(position)
        {
            self.anchor = None;
            self.head = None;
            self.fixed = Some((fragment_id, range));
            self.selecting = false;
            return;
        }
        self.fixed = None;
        self.anchor = Some(position);
        self.head = Some(position);
        self.selecting = true;
    }

    pub(crate) fn update(&mut self, position: Point<Pixels>) {
        if self.selecting {
            self.head = Some(position);
        }
    }

    pub(crate) fn finish(&mut self) {
        self.selecting = false;
    }

    pub(crate) fn is_selecting(&self) -> bool {
        self.selecting && self.group.is_active(&self.id)
    }

    pub(crate) fn endpoints(&self) -> Option<(Point<Pixels>, Point<Pixels>)> {
        self.group
            .is_active(&self.id)
            .then_some((self.anchor?, self.head?))
    }

    pub(crate) fn fixed_range(&self, fragment_id: &SharedString) -> Option<Range<usize>> {
        if !self.group.is_active(&self.id) {
            return None;
        }
        let (id, range) = self.fixed.as_ref()?;
        (id == fragment_id).then(|| range.clone())
    }

    pub(crate) fn record(
        &mut self,
        id: SharedString,
        bounds: Bounds<Pixels>,
        text: SharedString,
        range: Option<Range<usize>>,
        layout: TextLayout,
        highlights: Vec<Bounds<Pixels>>,
    ) {
        self.fragments.insert(
            id,
            SelectionFragment {
                generation: self.generation,
                bounds,
                text,
                range,
                layout,
                highlights,
            },
        );
    }

    pub(crate) fn selection_contains(&self, position: Point<Pixels>) -> bool {
        self.group.is_active(&self.id)
            && self
                .fragments
                .values()
                .filter(|fragment| fragment.generation == self.generation)
                .flat_map(|fragment| &fragment.highlights)
                .any(|bounds| bounds.contains(&position))
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.head = None;
        self.fixed = None;
        self.selecting = false;
    }

    fn word_at(&self, position: Point<Pixels>) -> Option<(SharedString, Range<usize>)> {
        let (id, fragment) = self.fragments.iter().find(|(_, fragment)| {
            fragment.generation == self.generation && fragment.bounds.contains(&position)
        })?;
        let index = match fragment.layout.index_for_position(position) {
            Ok(index) | Err(index) => index,
        }
        .min(fragment.text.len());
        let range = word_range(fragment.text.as_ref(), index)?;
        Some((id.clone(), range))
    }

    pub fn selected_text(&self) -> Option<String> {
        if !self.group.is_active(&self.id) {
            return None;
        }
        let mut fragments = self
            .fragments
            .values()
            .filter(|fragment| fragment.generation == self.generation)
            .filter_map(|fragment| {
                let range = fragment.range.clone()?;
                (!range.is_empty()).then_some((fragment, range))
            })
            .collect::<Vec<_>>();
        fragments.sort_by(|(a, _), (b, _)| {
            a.bounds
                .top()
                .partial_cmp(&b.bounds.top())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.bounds
                        .left()
                        .partial_cmp(&b.bounds.left())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut output = String::new();
        let mut previous: Option<&SelectionFragment> = None;
        for (fragment, range) in fragments {
            if let Some(previous) = previous {
                if fragment.bounds.top() >= previous.bounds.bottom() {
                    output.push('\n');
                } else if !output.ends_with(char::is_whitespace) {
                    output.push(' ');
                }
            }
            output.push_str(&fragment.text[range]);
            previous = Some(fragment);
        }
        (!output.is_empty()).then_some(output)
    }
}

fn word_range(text: &str, index: usize) -> Option<Range<usize>> {
    text.split_word_bound_indices().find_map(|(start, word)| {
        let end = start + word.len();
        (start <= index && index < end).then_some(start..end)
    })
}

#[cfg(test)]
mod tests {
    use super::{SelectionGroup, word_range};

    #[test]
    fn another_row_deactivates_the_previous_selection() {
        let group = SelectionGroup::new();
        group.activate("a".into());
        assert!(group.is_active(&"a".into()));
        group.activate("b".into());
        assert!(!group.is_active(&"a".into()));
    }

    #[test]
    fn word_range_uses_unicode_boundaries() {
        let text = "hello 世界 👋🏽";
        assert_eq!(&text[word_range(text, 1).unwrap()], "hello");
        let cjk = text.find('世').unwrap();
        assert!(!text[word_range(text, cjk).unwrap()].is_empty());
        let emoji = text.find('👋').unwrap();
        assert_eq!(&text[word_range(text, emoji).unwrap()], "👋🏽");
    }
}
