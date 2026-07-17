/// Single-assignment latch for competing terminal causes. The first candidate
/// becomes authoritative and later candidates cannot overwrite it.
pub(crate) struct TerminalSelector<T> {
    selected: Option<T>,
}

impl<T> TerminalSelector<T> {
    pub fn new() -> Self {
        Self { selected: None }
    }

    pub fn choose(&mut self, candidate: T) -> bool {
        if self.selected.is_some() {
            return false;
        }
        self.selected = Some(candidate);
        true
    }

    pub fn into_selected(self) -> Option<T> {
        self.selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_terminal_candidate_is_authoritative() {
        let mut selector = TerminalSelector::new();
        assert!(selector.choose("normal"));
        assert!(!selector.choose("cancel"));
        assert!(!selector.choose("panic"));
        assert_eq!(selector.into_selected(), Some("normal"));
    }
}
