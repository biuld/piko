//! Client-side projection store for per-agent todo lists.

use std::collections::HashMap;

use piko_protocol::TodoList;

/// Host-projected todo lists keyed by agent instance id.
#[derive(Clone, Debug, Default)]
pub struct TodoListsState {
    by_agent: HashMap<String, TodoList>,
}

impl TodoListsState {
    pub fn clear(&mut self) {
        self.by_agent.clear();
    }

    /// Replace all lists from a session snapshot.
    pub fn replace_all(&mut self, lists: impl IntoIterator<Item = TodoList>) {
        self.by_agent.clear();
        for list in lists {
            self.upsert(list);
        }
    }

    /// Upsert one agent's list (live `TodoListUpdated` or snapshot merge).
    pub fn upsert(&mut self, list: TodoList) {
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

    #[allow(dead_code)]
    pub fn get(&self, agent_instance_id: &str) -> Option<&TodoList> {
        self.by_agent.get(agent_instance_id)
    }
}
