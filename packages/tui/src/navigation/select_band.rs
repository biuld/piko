//! ComposerBand **content-row budgets** (not absolute band heights).
//!
//! Select surfaces declare how many **content** terminal rows they want as the
//! visible viewport. Overflow list items scroll. Compose converts
//! `chrome_rows + content_rows` → band cells and body-clamps.

/// Chrome for Minimal selectable list panes used by Models / Agents / Auth menu:
/// top border · search · footer · bottom border.
pub const MINIMAL_LIST_CHROME_ROWS: u16 = 4;

/// Chrome for Minimal form (no search): top · footer · bottom.
pub const MINIMAL_FORM_CHROME_ROWS: u16 = 3;

/// Chrome for a Standard pane with footer, no search (MCP-style):
/// all borders · vertical pad 1×2 · footer.
pub const STANDARD_INFO_CHROME_ROWS: u16 = 5;

/// Default cap: full items shown before list scrolling.
pub const DEFAULT_MAX_VISIBLE_ITEMS: u16 = 6;

/// Desired **content** viewport for a Select / ComposerBand surface.
///
/// Features declare row budgets; compose owns band sizing / body clamping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectBandBudget {
    /// Scrollable list. Content rows = `min(item_count, max_visible) * row_lines`
    /// (always a multiple of `row_lines` so multi-line items pack flush).
    List {
        item_count: usize,
        /// Lines per list item (`1` = single-line rows, `2` = Stacked).
        row_lines: u16,
        max_visible_items: u16,
        chrome_rows: u16,
    },
    /// Short form / prose: exact content rows, no item packing.
    Fixed { content_rows: u16, chrome_rows: u16 },
}

impl SelectBandBudget {
    /// Stacked selectable list (primary + detail) — auth menus.
    pub fn minimal_stacked_list(item_count: usize) -> Self {
        Self::List {
            item_count,
            row_lines: 2,
            max_visible_items: DEFAULT_MAX_VISIBLE_ITEMS,
            chrome_rows: MINIMAL_LIST_CHROME_ROWS,
        }
    }

    /// Single-line selectable list — Agents / Models (Columns body).
    pub fn minimal_dense_list(item_count: usize) -> Self {
        Self::List {
            item_count,
            row_lines: 1,
            max_visible_items: 8,
            chrome_rows: MINIMAL_LIST_CHROME_ROWS,
        }
    }

    /// Minimal pane form without a search row — Auth API-key prompt.
    pub fn minimal_form(content_rows: u16) -> Self {
        Self::Fixed {
            content_rows: content_rows.max(1),
            chrome_rows: MINIMAL_FORM_CHROME_ROWS,
        }
    }

    /// Standard info panel on ComposerBand — MCP status.
    pub fn standard_info(content_rows: u16) -> Self {
        Self::Fixed {
            content_rows: content_rows.max(1),
            chrome_rows: STANDARD_INFO_CHROME_ROWS,
        }
    }

    pub fn chrome_rows(self) -> u16 {
        match self {
            Self::List { chrome_rows, .. } | Self::Fixed { chrome_rows, .. } => chrome_rows,
        }
    }

    /// Visible content rows the feature wants (pre-clamp).
    pub fn content_rows(self) -> u16 {
        match self {
            Self::List {
                item_count,
                row_lines,
                max_visible_items,
                ..
            } => {
                let rl = row_lines.max(1);
                let visible = (item_count as u16).min(max_visible_items.max(1)).max(1); // empty list still reserves one item slot
                visible.saturating_mul(rl)
            }
            Self::Fixed { content_rows, .. } => content_rows.max(1),
        }
    }

    pub fn preferred_band_rows(self) -> u16 {
        self.chrome_rows().saturating_add(self.content_rows())
    }

    /// Smallest useful band (chrome + one full item / minimal content).
    pub fn min_band_rows(self) -> u16 {
        let content = match self {
            Self::List { row_lines, .. } => row_lines.max(1),
            Self::Fixed { content_rows, .. } => content_rows.max(1),
        };
        self.chrome_rows().saturating_add(content)
    }

    /// Prefer this budget as a band height, body-clamped, list-aligned.
    pub fn resolve_band_rows(self, body_height: u16) -> u16 {
        let max = body_height.saturating_sub(4).max(self.min_band_rows());
        let min = self.min_band_rows().min(max);
        let preferred = self.preferred_band_rows();
        let band = preferred.clamp(min, max);
        self.align_band(band).clamp(min, max)
    }

    /// Floor content height to a multiple of `row_lines` so List does not leave
    /// a half-item gap above the footer.
    fn align_band(self, band: u16) -> u16 {
        match self {
            Self::List {
                row_lines,
                chrome_rows,
                ..
            } => {
                let rl = row_lines.max(1);
                let mut content = band.saturating_sub(chrome_rows);
                content = (content / rl) * rl;
                if content < rl {
                    content = rl;
                }
                chrome_rows.saturating_add(content)
            }
            Self::Fixed { .. } => band,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_content_rows_are_even() {
        let b = SelectBandBudget::minimal_stacked_list(3);
        assert_eq!(b.content_rows(), 6); // 3 items × 2
        assert_eq!(b.preferred_band_rows(), 10); // 4 chrome + 6
    }

    #[test]
    fn stacked_caps_visible_items() {
        let b = SelectBandBudget::minimal_stacked_list(100);
        assert_eq!(b.content_rows(), DEFAULT_MAX_VISIBLE_ITEMS * 2);
    }

    #[test]
    fn empty_list_reserves_one_slot() {
        let b = SelectBandBudget::minimal_stacked_list(0);
        assert_eq!(b.content_rows(), 2);
    }

    #[test]
    fn dense_list_one_line_items() {
        let b = SelectBandBudget::minimal_dense_list(5);
        assert_eq!(b.content_rows(), 5);
    }

    #[test]
    fn resolve_clamps_to_body_and_stays_aligned() {
        let b = SelectBandBudget::minimal_stacked_list(40);
        // preferred = 4 + 12 = 16; tiny body forces clamp + keep even content.
        let band = b.resolve_band_rows(12);
        assert!(band <= 12);
        let content = band.saturating_sub(MINIMAL_LIST_CHROME_ROWS);
        assert_eq!(content % 2, 0);
        assert!(content >= 2);
    }
}
