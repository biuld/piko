//! Active-branch walk over session tree entries (TUI-compatible).

use piko_protocol::session::SessionTreeEntry;

/// Return entries on the path from `current_leaf_id` to the root (chronological).
///
/// If `current_leaf_id` is absent, return all entries as-is.
pub fn active_branch_entries(
    entries: &[SessionTreeEntry],
    current_leaf_id: Option<&str>,
) -> Vec<SessionTreeEntry> {
    let Some(leaf_id) = current_leaf_id else {
        return entries.to_vec();
    };

    let mut by_id = std::collections::HashMap::new();
    for entry in entries {
        by_id.insert(entry.id(), entry);
    }

    let mut path = Vec::new();
    let mut curr_id = Some(leaf_id.to_string());
    let mut visited = std::collections::HashSet::new();

    while let Some(id) = curr_id {
        if !visited.insert(id.clone()) {
            break;
        }
        if let Some(entry) = by_id.get(id.as_str()) {
            path.push((*entry).clone());
            curr_id = entry.parent_id().map(str::to_string);
        } else {
            break;
        }
    }

    path.reverse();
    path
}

/// Return entry ids on the active path from leaf to root (chronological order).
pub fn active_path_ids(entries: &[SessionTreeEntry], current_leaf_id: Option<&str>) -> Vec<String> {
    active_branch_entries(entries, current_leaf_id)
        .into_iter()
        .map(|e| e.id().to_string())
        .collect()
}
