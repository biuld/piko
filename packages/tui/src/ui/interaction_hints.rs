//! Placement-independent contract for contextual interaction guidance.

/// Passive guidance declared by the feature that owns the active interaction.
///
/// This value describes content only. Layout and rendering policy decide
/// whether it appears in a pane footer, a shared row, or another projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteractionHints<'a> {
    text: &'a str,
}

impl<'a> InteractionHints<'a> {
    pub const fn new(text: &'a str) -> Self {
        Self { text }
    }

    /// First non-empty line for compact, single-row projections.
    pub fn single_line(self) -> Option<&'a str> {
        self.text.lines().find(|line| !line.trim().is_empty())
    }

    pub fn is_empty(self) -> bool {
        self.single_line().is_none()
    }
}

impl<'a> From<&'a str> for InteractionHints<'a> {
    fn from(text: &'a str) -> Self {
        Self::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::InteractionHints;

    #[test]
    fn compact_projection_selects_first_non_empty_line() {
        let hints = InteractionHints::new("\n  \nEnter confirm\nEsc cancel");
        assert_eq!(hints.single_line(), Some("Enter confirm"));
    }

    #[test]
    fn whitespace_only_content_is_empty() {
        assert!(InteractionHints::new("\n \t\n").is_empty());
    }
}
