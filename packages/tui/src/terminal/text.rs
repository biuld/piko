//! One grapheme/terminal-column policy shared by editor and presentation.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TerminalTextPolicy;

impl TerminalTextPolicy {
    pub fn width(self, text: &str) -> usize {
        UnicodeWidthStr::width(text)
    }

    pub fn graphemes(self, text: &str) -> impl Iterator<Item = &str> {
        text.graphemes(true)
    }

    pub fn grapheme_indices(self, text: &str) -> impl Iterator<Item = (usize, &str)> {
        text.grapheme_indices(true)
    }

    pub fn previous_grapheme_boundary(self, text: &str, cursor: usize) -> Option<usize> {
        let cursor = valid_boundary_at_or_before(text, cursor);
        self.grapheme_indices(&text[..cursor])
            .last()
            .map(|(index, _)| index)
    }

    pub fn next_grapheme_boundary(self, text: &str, cursor: usize) -> Option<usize> {
        let cursor = valid_boundary_at_or_before(text, cursor);
        self.grapheme_indices(&text[cursor..])
            .nth(1)
            .map(|(index, _)| cursor + index)
            .or_else(|| (cursor < text.len()).then_some(text.len()))
    }

    pub fn prefix(self, text: &str, max_cols: usize) -> (&str, usize) {
        if max_cols == 0 {
            return (&text[..0], 0);
        }
        let mut used: usize = 0;
        for (index, grapheme) in text.grapheme_indices(true) {
            let width = self.width(grapheme);
            if used.saturating_add(width) > max_cols {
                return (&text[..index], used);
            }
            used = used.saturating_add(width);
        }
        (text, used)
    }

    pub fn truncate(self, text: &str, max_cols: usize) -> String {
        self.prefix(text, max_cols).0.to_string()
    }

    pub fn soft_wrap(self, text: &str, max_cols: usize) -> Vec<String> {
        let max_cols = max_cols.max(1);
        let mut rows = Vec::new();
        let mut row = String::new();
        let mut used = 0usize;
        for (segment_index, segment) in text.split('\n').enumerate() {
            if segment_index > 0 {
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
            for grapheme in self.graphemes(segment) {
                let width = self.width(grapheme);
                if used > 0 && used.saturating_add(width) > max_cols {
                    rows.push(std::mem::take(&mut row));
                    used = 0;
                }
                row.push_str(grapheme);
                used = used.saturating_add(width);
            }
        }
        if !row.is_empty() || rows.is_empty() {
            rows.push(row);
        }
        rows
    }
}

pub fn display_width(text: &str) -> usize {
    TerminalTextPolicy.width(text)
}

fn valid_boundary_at_or_before(text: &str, cursor: usize) -> usize {
    let mut boundary = cursor.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_and_wrap_keep_graphemes_whole() {
        let policy = TerminalTextPolicy;
        let family = "👨‍👩‍👧‍👦";
        let input = format!("{family}x");
        let (prefix, used) = policy.prefix(&input, 2);
        assert_eq!(prefix, family);
        assert_eq!(used, policy.width(family));
        assert_eq!(&input[prefix.len()..], "x");
        assert_eq!(policy.soft_wrap("你a你", 3), vec!["你a", "你"]);
        assert_eq!(policy.soft_wrap("a\n\nb", 3), vec!["a", "", "b"]);
    }

    #[test]
    fn grapheme_boundaries_are_used_for_editor_carets() {
        let policy = TerminalTextPolicy;
        let text = "e\u{301}x";
        assert_eq!(policy.previous_grapheme_boundary(text, text.len()), Some(3));
        assert_eq!(policy.previous_grapheme_boundary(text, 3), Some(0));
        assert_eq!(policy.next_grapheme_boundary(text, 0), Some(3));
        assert_eq!(policy.next_grapheme_boundary(text, 3), Some(4));
    }
}
