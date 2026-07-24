//! Parsed Markdown cache owned by the Timeline projection.

use std::collections::{HashMap, HashSet};

use piko_chrome::components::markdown::{MarkdownDocument, parse_markdown};

use super::vm::TimelineRow;

#[derive(Default)]
pub(super) struct TimelineMarkdownCache {
    entries: HashMap<String, CachedMarkdown>,
}

struct CachedMarkdown {
    source: String,
    document: MarkdownDocument,
}

impl TimelineMarkdownCache {
    pub(super) fn sync(&mut self, rows: &[TimelineRow]) {
        let markdown_ids = rows
            .iter()
            .filter(|row| row.render_markdown)
            .map(|row| row.id.as_str())
            .collect::<HashSet<_>>();
        self.entries
            .retain(|row_id, _| markdown_ids.contains(row_id.as_str()));

        for row in rows.iter().filter(|row| row.render_markdown) {
            let is_current = self
                .entries
                .get(&row.id)
                .is_some_and(|cached| cached.source == row.body);
            if !is_current {
                self.entries.insert(
                    row.id.clone(),
                    CachedMarkdown {
                        source: row.body.clone(),
                        document: parse_markdown(&row.body),
                    },
                );
            }
        }
    }

    pub(super) fn document(&self, row_id: &str) -> Option<&MarkdownDocument> {
        self.entries.get(row_id).map(|cached| &cached.document)
    }
}
