//! Window-local UX preferences (not persisted this wave).

#[derive(Debug, Clone, Default)]
pub struct GuiUxPrefs {
    /// When true, skip decorative animations / spinners.
    pub prefer_reduced_motion: bool,
}

impl GuiUxPrefs {
    /// Whether decorative motion should run.
    pub fn allow_motion(&self) -> bool {
        !self.prefer_reduced_motion
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
}
