//! Welcome ASCII wordmarks, sparkle frames, and logo-style cycling.
//!
//! Keep rows equal visual width within each logo set. Animation timing lives
//! here so render only consumes [`LogoStyle`] / glyph helpers.

/// Slow logo-style cycle: ~3.2s at 80ms app ticks.
pub(super) const STYLE_TICKS: usize = 40;
/// Highlight wave period across logo rows.
pub(super) const WAVE_TICKS: usize = 6;

/// Curated ASCII wordmarks for "piko".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LogoStyle {
    /// Classic FIGlet slant — default brand mark.
    Slant,
    /// Compact box-drawing mark for medium widths.
    Box,
    /// Dense block glyphs when the terminal is wide enough.
    Blocks,
}

impl LogoStyle {
    pub(super) fn cycle(frame: usize) -> Self {
        match (frame / STYLE_TICKS) % 3 {
            0 => Self::Slant,
            1 => Self::Box,
            _ => Self::Blocks,
        }
    }

    pub(super) fn rows(self) -> &'static [&'static str] {
        match self {
            Self::Slant => LOGO_SLANT,
            Self::Box => LOGO_BOX,
            Self::Blocks => LOGO_BLOCKS,
        }
    }

    pub(super) fn min_width(self) -> u16 {
        self.rows()
            .iter()
            .map(|r| r.chars().count() as u16)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn height(self) -> usize {
        self.rows().len()
    }
}

/// All logo styles that may appear in the cycle / fallback order.
pub(super) const ALL_STYLES: [LogoStyle; 3] = [LogoStyle::Slant, LogoStyle::Box, LogoStyle::Blocks];

/// Fixed logo band height — tall enough for every style so the welcome card
/// does not resize when the animated style changes.
pub(super) fn logo_region_height() -> usize {
    ALL_STYLES
        .iter()
        .map(|s| s.height())
        .max()
        .unwrap_or(1)
        .max(1)
}

/// Fixed logo band width — widest style; narrower logos are centered inside.
pub(super) fn logo_region_width() -> u16 {
    ALL_STYLES
        .iter()
        .map(|s| s.min_width())
        .max()
        .unwrap_or(1)
        .max(1)
}

// ── Logo art ─────────────────────────────────────────────────────────────────

const LOGO_SLANT: &[&str] = &[
    r"      _ __        ",
    r" ___ (_) /______  ",
    r"/ _ \/ /  '_/ __ \",
    r"/ .__/_/_/\_\____/",
    r"/_/               ",
];

const LOGO_BOX: &[&str] = &["┌─┐ ┬ ┬┌─┌─┐", "├─┘ │┌┘│ │ │", "┴   ┴└─└─┘─┘"];

const LOGO_BLOCKS: &[&str] = &["█▀█ █ █▄▀ █▀█", "█▀▀ █ █ █ █▄█"];

/// Narrow-terminal wordmark sparkle cycle.
const SPARKLE_FRAMES: &[&str] = &["·", "✧", "·", "✦", "·", " "];

pub(super) fn sparkle_glyph(frame: usize) -> &'static str {
    SPARKLE_FRAMES[frame % SPARKLE_FRAMES.len()]
}

/// Prefer the cycled style when it fits; otherwise the widest style that fits.
pub(super) fn pick_logo_style(spinner_frame: usize, available_width: u16) -> LogoStyle {
    let preferred = LogoStyle::cycle(spinner_frame);
    if available_width >= preferred.min_width() {
        return preferred;
    }
    for style in [LogoStyle::Box, LogoStyle::Slant, LogoStyle::Blocks] {
        if available_width >= style.min_width() {
            return style;
        }
    }
    preferred
}

/// Logo rows padded to [`logo_region_height`], vertically centered in the band.
///
/// Glyph text is not width-padded here; callers center each non-empty row inside
/// the fixed logo band width / card content width.
pub(super) fn logo_band_rows(style: LogoStyle) -> Vec<&'static str> {
    let region_h = logo_region_height();
    let rows = style.rows();
    let top = region_h.saturating_sub(rows.len()) / 2;
    let mut band = vec![""; region_h];
    for (i, row) in rows.iter().enumerate() {
        if let Some(slot) = band.get_mut(top + i) {
            *slot = row;
        }
    }
    band
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_style_cycles_with_spinner() {
        assert_eq!(LogoStyle::cycle(0), LogoStyle::Slant);
        assert_eq!(LogoStyle::cycle(STYLE_TICKS), LogoStyle::Box);
        assert_eq!(LogoStyle::cycle(STYLE_TICKS * 2), LogoStyle::Blocks);
        assert_eq!(LogoStyle::cycle(STYLE_TICKS * 3), LogoStyle::Slant);
    }

    #[test]
    fn narrow_pick_prefers_fitting_style() {
        let style = pick_logo_style(STYLE_TICKS * 2, 14);
        assert!(style.min_width() <= 14, "picked style too wide: {style:?}");
    }

    #[test]
    fn all_logo_styles_have_uniform_row_widths() {
        for style in ALL_STYLES {
            let rows = style.rows();
            let widths: Vec<usize> = rows.iter().map(|r| r.chars().count()).collect();
            let first = widths[0];
            assert!(
                widths.iter().all(|&w| w == first),
                "{style:?} rows uneven: {widths:?}"
            );
        }
    }

    #[test]
    fn logo_band_height_is_stable_across_styles() {
        let h = logo_region_height();
        for style in ALL_STYLES {
            assert_eq!(
                logo_band_rows(style).len(),
                h,
                "{style:?} band height drifted"
            );
        }
    }

    #[test]
    fn sparkle_cycles() {
        assert_eq!(sparkle_glyph(0), SPARKLE_FRAMES[0]);
        assert_eq!(sparkle_glyph(SPARKLE_FRAMES.len()), SPARKLE_FRAMES[0]);
    }
}
