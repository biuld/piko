//! Window-local UX preferences (not persisted this wave).

use island::theme::IslandPalette;

#[derive(Debug, Clone)]
pub struct GuiUxPrefs {
    /// When true, skip decorative animations / spinners.
    pub prefer_reduced_motion: bool,
    /// When true, hide thinking/reasoning blocks in the timeline.
    /// GUI-only; independent of the TUI's own `[tui].hide_thinking_block`.
    pub hide_thinking_block: bool,
    /// Active chrome palette (dark / light).
    pub island_palette: IslandPalette,
}

impl Default for GuiUxPrefs {
    fn default() -> Self {
        Self {
            prefer_reduced_motion: false,
            hide_thinking_block: false,
            island_palette: IslandPalette::Dark,
        }
    }
}

impl GuiUxPrefs {
    /// Whether decorative motion should run.
    pub fn allow_motion(&self) -> bool {
        !self.prefer_reduced_motion
    }
}

/// Parse a persisted `[gui].island-palette` string.
pub fn parse_island_palette(raw: &str) -> IslandPalette {
    match raw.trim().to_ascii_lowercase().as_str() {
        "light" => IslandPalette::Light,
        _ => IslandPalette::Dark,
    }
}

pub fn island_palette_key(palette: IslandPalette) -> &'static str {
    match palette {
        IslandPalette::Dark => "dark",
        IslandPalette::Light => "light",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_disables_allow_motion() {
        let mut prefs = GuiUxPrefs::default();
        assert!(prefs.allow_motion());
        prefs.prefer_reduced_motion = true;
        assert!(!prefs.allow_motion());
    }

    #[test]
    fn parse_island_palette_accepts_light_and_defaults_dark() {
        assert_eq!(parse_island_palette("light"), IslandPalette::Light);
        assert_eq!(parse_island_palette("LIGHT"), IslandPalette::Light);
        assert_eq!(parse_island_palette("dark"), IslandPalette::Dark);
        assert_eq!(parse_island_palette("unknown"), IslandPalette::Dark);
    }
}
