//! Parsed Markdown cache owned by the Timeline projection.

use std::collections::{HashMap, HashSet};

use gpui::{App, AppContext, Entity};
use piko_chrome::components::markdown::{MarkdownDocument, parse_markdown};
use piko_chrome::components::selection::{SelectionGroup, SelectionState};

use super::vm::TimelineRow;

pub(super) struct TimelineMarkdownCache {
    entries: HashMap<String, CachedMarkdown>,
    selection_group: SelectionGroup,
    owner: gpui::EntityId,
}

struct CachedMarkdown {
    source: String,
    document: Option<MarkdownDocument>,
    selection: Entity<SelectionState>,
}

impl TimelineMarkdownCache {
    pub(super) fn new(owner: gpui::EntityId) -> Self {
        Self {
            entries: HashMap::new(),
            selection_group: SelectionGroup::new(),
            owner,
        }
    }

    pub(super) fn sync(&mut self, rows: &[TimelineRow], cx: &mut App) {
        let row_ids = rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<HashSet<_>>();
        self.entries
            .retain(|row_id, _| row_ids.contains(row_id.as_str()));

        for row in rows {
            let is_current = self
                .entries
                .get(&row.id)
                .is_some_and(|cached| cached.source == row.body);
            if !is_current {
                let document = row.render_markdown.then(|| parse_markdown(&row.body));
                if let Some(cached) = self.entries.get_mut(&row.id) {
                    if !row.body.starts_with(&cached.source) {
                        cached.selection.update(cx, |state, _| state.clear());
                    }
                    cached.source = row.body.clone();
                    cached.document = document;
                } else {
                    let id = row.id.clone();
                    let group = self.selection_group.clone();
                    let selection = cx.new(|cx| SelectionState::new(id, group, cx));
                    self.entries.insert(
                        row.id.clone(),
                        CachedMarkdown {
                            source: row.body.clone(),
                            document,
                            selection,
                        },
                    );
                }
            }
        }
    }

    pub(super) fn document(&self, row_id: &str) -> Option<&MarkdownDocument> {
        self.entries
            .get(row_id)
            .and_then(|cached| cached.document.as_ref())
    }

    pub(super) fn selection(&self, row_id: &str) -> Option<Entity<SelectionState>> {
        self.entries
            .get(row_id)
            .map(|cached| cached.selection.clone())
    }

    pub(super) fn selected_text(&self, cx: &App) -> Option<String> {
        self.entries
            .values()
            .find_map(|cached| cached.selection.read(cx).selected_text())
    }

    pub(super) fn owner(&self) -> gpui::EntityId {
        self.owner
    }
}
