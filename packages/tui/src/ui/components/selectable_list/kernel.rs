/// Selection state shared by menus, sessions, settings, and suggestion palettes.
pub struct SelectableList<T> {
    pub items: Vec<T>,
    pub selected: usize,
}

impl<T> Default for SelectableList<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T> SelectableList<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self { items, selected: 0 }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.selected = 0;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Select a source row when it exists. Returns whether selection changed.
    pub fn select_index(&mut self, index: usize) -> bool {
        if index >= self.items.len() || self.selected == index {
            return false;
        }
        self.selected = index;
        true
    }

    /// Move one row with wraparound for compact suggestion palettes.
    pub fn select_next_wrapped(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    /// Move one row with wraparound for compact suggestion palettes.
    pub fn select_prev_wrapped(&mut self) {
        if !self.items.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.items.len() - 1);
        }
    }

    /// Select first match under `filter` (or 0 if none).
    pub fn reset_selection<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        self.selected = filtered.first().copied().unwrap_or(0);
    }

    pub fn filtered_indices<F>(&self, filter: &str, mut f: F) -> Vec<usize>
    where
        F: FnMut(&T) -> bool,
    {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| if filter.is_empty() { true } else { f(item) })
            .map(|(idx, _)| idx)
            .collect()
    }

    pub fn select_next<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        if filtered.is_empty() {
            return;
        }
        let current = filtered
            .iter()
            .position(|&index| index == self.selected)
            .unwrap_or(0);
        if let Some(&index) = filtered.get((current + 1).min(filtered.len() - 1)) {
            self.selected = index;
        }
    }

    pub fn select_prev<F>(&mut self, filter: &str, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let filtered = self.filtered_indices(filter, f);
        if filtered.is_empty() {
            return;
        }
        let current = filtered
            .iter()
            .position(|&index| index == self.selected)
            .unwrap_or(0);
        if let Some(&index) = filtered.get(current.saturating_sub(1)) {
            self.selected = index;
        }
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.items.get(self.selected)
    }
}
