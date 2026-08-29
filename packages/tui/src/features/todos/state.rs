//! Client-side projection store for per-agent todo lists.

use std::cell::Cell;
use std::collections::HashMap;

use piko_protocol::TodoList;

/// Host-projected todo lists keyed by agent instance id.
#[derive(Clone, Debug)]
pub struct TodoListsState {
    by_agent: HashMap<String, TodoList>,
    /// Item window offset into the viewed list (0 = top).
    scroll: Cell<usize>,
    /// Max scroll for the last painted grant; set by paint so wheel events
    /// (which only see `&AppState`) can clamp against the real viewport.
    max_scroll: Cell<usize>,
}

impl Default for TodoListsState {
    fn default() -> Self {
        Self {
            by_agent: HashMap::new(),
            scroll: Cell::new(0),
            max_scroll: Cell::new(0),
        }
    }
}

impl TodoListsState {
    pub fn clear(&mut self) {
        self.by_agent.clear();
        self.scroll.set(0);
        self.max_scroll.set(0);
    }

    /// Replace all lists from a session snapshot.
    pub fn replace_all(&mut self, lists: impl IntoIterator<Item = TodoList>) {
        self.by_agent.clear();
        self.scroll.set(0);
        for list in lists {
            self.upsert(list);
        }
    }

    /// Upsert one agent's list (live `TodoListUpdated` or snapshot merge).
    pub fn upsert(&mut self, list: TodoList) {
        self.scroll.set(0);
        let id = list.agent_instance_id.clone();
        if list.items.is_empty() {
            self.by_agent.remove(&id);
        } else {
            self.by_agent.insert(id, list);
        }
    }

    /// List for the viewed agent when the feature is on and items non-empty.
    pub fn viewed_list(&self, viewed_agent: Option<&str>, feature_on: bool) -> Option<&TodoList> {
        if !feature_on {
            return None;
        }
        let id = viewed_agent?;
        self.by_agent.get(id).filter(|list| !list.items.is_empty())
    }

    /// Wheel-scroll the item window up by `amount` rows.
    pub fn scroll_up(&self, amount: usize) {
        self.scroll.set(self.scroll.get().saturating_sub(amount));
    }

    /// Wheel-scroll the item window down by `amount` rows, clamped to the
    /// viewport recorded by the last paint.
    pub fn scroll_down(&self, amount: usize) {
        self.scroll.set(
            self.scroll
                .get()
                .saturating_add(amount)
                .min(self.max_scroll.get()),
        );
    }

    /// Current item offset into the viewed list (0 = top).
    pub fn scroll_offset(&self) -> usize {
        self.scroll.get().min(self.max_scroll.get())
    }

    /// Record the painted viewport and clamp the stored offset to it.
    pub fn set_max_scroll(&self, max: usize) {
        self.max_scroll.set(max);
        self.scroll.set(self.scroll.get().min(max));
    }

    pub fn reset_scroll(&self) {
        self.scroll.set(0);
        self.max_scroll.set(0);
    }

    #[allow(dead_code)]
    pub fn get(&self, agent_instance_id: &str) -> Option<&TodoList> {
        self.by_agent.get(agent_instance_id)
    }
}
