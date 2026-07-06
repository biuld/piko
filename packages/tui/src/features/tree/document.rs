use piko_protocol::SessionTreeEntry;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct TreeLabel {
    pub text: Option<String>,
    pub timestamp: String,
}

#[derive(Default, Debug)]
pub struct TreeDocument {
    pub nodes: Vec<SessionTreeEntry>,
    pub by_id: HashMap<String, usize>,
    pub children_by_parent: HashMap<Option<String>, Vec<String>>,
    pub labels_by_target: HashMap<String, TreeLabel>,
    pub active_path: HashSet<String>,
    pub current_leaf_id: Option<String>,
}

impl TreeDocument {
    pub fn build(entries: &[SessionTreeEntry], current_leaf_id: Option<&str>) -> Self {
        let mut doc = Self {
            nodes: entries.to_vec(),
            current_leaf_id: current_leaf_id.map(str::to_string),
            ..Default::default()
        };

        for (idx, entry) in entries.iter().enumerate() {
            let id = entry.id().to_string();
            doc.by_id.insert(id.clone(), idx);
            let parent_id = entry.parent_id().map(str::to_string);
            doc.children_by_parent
                .entry(parent_id)
                .or_default()
                .push(id.clone());

            if let SessionTreeEntry::Label(l) = entry {
                doc.labels_by_target.insert(
                    l.target_id.clone(),
                    TreeLabel {
                        text: l.label.clone(),
                        timestamp: l.timestamp.clone(),
                    },
                );
            }
        }

        let mut curr = doc.current_leaf_id.clone();
        let mut visited = HashSet::new();
        while let Some(id) = curr {
            if !visited.insert(id.clone()) {
                break; // cycle detected (e.g. id == parentId)
            }
            doc.active_path.insert(id.clone());
            if let Some(&idx) = doc.by_id.get(&id) {
                curr = entries[idx].parent_id().map(str::to_string);
            } else {
                break;
            }
        }

        doc
    }
}
