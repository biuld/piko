//! Window-local UX preferences (not persisted this wave).

use piko_chrome::theme::ChromePalette;

#[derive(Debug, Clone)]
pub struct GuiUxPrefs {
    /// When true, skip decorative animations / spinners.
    pub prefer_reduced_motion: bool,
    /// When true, hide thinking/reasoning blocks in the timeline.
    /// GUI-only; independent of the TUI's own `[tui].hide_thinking_block`.
    pub hide_thinking_block: bool,
    /// Active chrome palette (dark / light).
    pub chrome_palette: ChromePalette,
}

impl Default for GuiUxPrefs {
    fn default() -> Self {
        Self {
            prefer_reduced_motion: false,
            hide_thinking_block: false,
            chrome_palette: ChromePalette::Dark,
        }
    }
}

impl GuiUxPrefs {
    /// Whether decorative motion should run.
    pub fn allow_motion(&self) -> bool {
        !self.prefer_reduced_motion
    }
}

/// Parse a persisted `[gui].chrome-palette` string.
pub fn parse_chrome_palette(raw: &str) -> ChromePalette {
    match raw.trim().to_ascii_lowercase().as_str() {
        "light" => ChromePalette::Light,
        _ => ChromePalette::Dark,
    }
}

pub fn chrome_palette_key(palette: ChromePalette) -> &'static str {
    match palette {
        ChromePalette::Dark => "dark",
        ChromePalette::Light => "light",
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
    fn parse_chrome_palette_accepts_light_and_defaults_dark() {
        assert_eq!(parse_chrome_palette("light"), ChromePalette::Light);
        assert_eq!(parse_chrome_palette("LIGHT"), ChromePalette::Light);
        assert_eq!(parse_chrome_palette("dark"), ChromePalette::Dark);
        assert_eq!(parse_chrome_palette("unknown"), ChromePalette::Dark);
    }
}
