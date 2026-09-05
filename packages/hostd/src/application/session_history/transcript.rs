use std::collections::{BTreeMap, HashSet};

use piko_protocol::{
    HistoryItemKind, HistoryItemRef, HistoryTranscriptItem, HistoryTranscriptPage,
};
use piko_session_store::{HistoryEvent, InspectionBundle, StoredMessage, StoredTreeEntry};

pub(super) fn transcript_page(
    session_id: &str,
    bundle: &InspectionBundle,
    offset: usize,
    limit: usize,
) -> HistoryTranscriptPage {
    let on_branch = selected_branch(
        &bundle.current.tree_entries,
        bundle.current.selected_tree_entry_id.as_deref(),
    );
    let mut items = Vec::new();
    walk_tree(bundle, None, 0, &on_branch, &mut items);

    let in_tree = bundle
        .current
        .tree_entries
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    walk_private_messages(bundle, &in_tree, &mut items);
    walk_branch_selections(bundle, &mut items);

    let prefix = "transcript";
    let (items, next_cursor) = super::page(items, offset, limit, prefix, bundle.revision);
    HistoryTranscriptPage {
        session_id: session_id.to_string(),
        revision: bundle.revision,
        items,
        next_cursor,
    }
}

fn selected_branch(
    tree: &BTreeMap<String, StoredTreeEntry>,
    selected: Option<&str>,
) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut current = selected.map(str::to_string);
    while let Some(id) = current {
        let parent = tree
            .get(&id)
            .and_then(|entry| entry.data.parent_entry_id.clone());
        ids.insert(id);
        current = parent;
    }
    ids
}

fn walk_tree(
    bundle: &InspectionBundle,
    parent: Option<&str>,
    depth: u32,
    on_branch: &HashSet<String>,
    items: &mut Vec<HistoryTranscriptItem>,
) {
    let mut children = bundle
        .current
        .tree_entries
        .values()
        .filter(|entry| entry.data.parent_entry_id.as_deref() == parent)
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| (entry.data.timestamp, entry.data.entry_id.as_str()));
    for entry in children {
        if let Some(item) = tree_item(bundle, entry, depth, on_branch) {
            items.push(item);
        }
        walk_tree(
            bundle,
            Some(entry.data.entry_id.as_str()),
            depth + 1,
            on_branch,
            items,
        );
    }
}

fn tree_item(
    bundle: &InspectionBundle,
    entry: &StoredTreeEntry,
    depth: u32,
    on_branch: &HashSet<String>,
) -> Option<HistoryTranscriptItem> {
    let (item_ref, _, _) = locate_event(bundle, |event| {
        event.event_type == "tree_entry_recorded"
            && event.entity_id.as_deref() == Some(entry.data.entry_id.as_str())
    })?;
    let selected =
        bundle.current.selected_tree_entry_id.as_deref() == Some(entry.data.entry_id.as_str());
    let off_branch = !on_branch.is_empty() && !on_branch.contains(&entry.data.entry_id);
    let mut summary = format!("{} tree entry", entry.data.entry_type);
    if selected {
        summary.push_str(" · selected");
    } else if off_branch {
        summary.push_str(" · off-branch");
    }
    Some(HistoryTranscriptItem {
        item_ref,
        kind: HistoryItemKind::new("tree_entry"),
        depth,
        agent_instance_id: None,
        parent_id: entry.data.parent_entry_id.clone(),
        root_input_id: None,
        model_step_id: None,
        summary,
        selected,
        off_branch,
        has_detail: true,
    })
}

fn walk_private_messages(
    bundle: &InspectionBundle,
    in_tree: &HashSet<String>,
    items: &mut Vec<HistoryTranscriptItem>,
) {
    let private = bundle
        .current
        .messages
        .values()
        .filter(|message| !in_tree.contains(&message.data.message_id))
        .map(|message| (message.data.message_id.clone(), message))
        .collect::<BTreeMap<_, _>>();
    let mut by_parent: BTreeMap<Option<String>, Vec<&StoredMessage>> = BTreeMap::new();
    for message in private.values().copied() {
        let parent = message
            .data
            .agent_parent_message_id
            .clone()
            .filter(|id| private.contains_key(id));
        by_parent.entry(parent).or_default().push(message);
    }
    for children in by_parent.values_mut() {
        children
            .sort_by_key(|message| (message.data.committed_at, message.data.message_id.as_str()));
    }
    walk_message_children(bundle, &by_parent, None, 0, items);
}

fn walk_message_children(
    bundle: &InspectionBundle,
    by_parent: &BTreeMap<Option<String>, Vec<&StoredMessage>>,
    parent: Option<&str>,
    depth: u32,
    items: &mut Vec<HistoryTranscriptItem>,
) {
    let Some(children) = by_parent.get(&parent.map(str::to_string)) else {
        return;
    };
    for message in children {
        if let Some(item) = message_item(bundle, message, depth) {
            items.push(item);
        }
        walk_message_children(
            bundle,
            by_parent,
            Some(message.data.message_id.as_str()),
            depth + 1,
            items,
        );
    }
}

fn message_item(
    bundle: &InspectionBundle,
    message: &StoredMessage,
    depth: u32,
) -> Option<HistoryTranscriptItem> {
    let (item_ref, _, _) = locate_event(bundle, |event| {
        event.event_type == "message_committed"
            && event.entity_id.as_deref() == Some(message.data.message_id.as_str())
    })?;
    let selected = bundle
        .current
        .agent_heads
        .get(&message.data.agent_instance_id)
        .is_some_and(|head| head == &message.data.message_id);
    Some(HistoryTranscriptItem {
        item_ref,
        kind: HistoryItemKind::new("message"),
        depth,
        agent_instance_id: Some(message.data.agent_instance_id.clone()),
        parent_id: message.data.agent_parent_message_id.clone(),
        root_input_id: message.data.root_input_id.clone(),
        model_step_id: bundle
            .history
            .message_to_step
            .get(&message.data.message_id)
            .cloned(),
        summary: format!("{} message", message.data.message.role()),
        selected,
        off_branch: false,
        has_detail: true,
    })
}

fn walk_branch_selections(bundle: &InspectionBundle, items: &mut Vec<HistoryTranscriptItem>) {
    for commit in &bundle.history.commits {
        for (index, event) in commit.events.iter().enumerate() {
            if event.event_type != "branch_selected" {
                continue;
            }
            items.push(HistoryTranscriptItem {
                item_ref: HistoryItemRef {
                    revision: bundle.revision,
                    token: format!("event:{}:{index}", commit.revision),
                },
                kind: HistoryItemKind::new("branch_selected"),
                depth: 0,
                agent_instance_id: None,
                parent_id: None,
                root_input_id: None,
                model_step_id: None,
                summary: event.summary.clone(),
                selected: false,
                off_branch: false,
                has_detail: true,
            });
        }
    }
}

fn locate_event(
    bundle: &InspectionBundle,
    pred: impl Fn(&HistoryEvent) -> bool,
) -> Option<(HistoryItemRef, u64, u32)> {
    for commit in &bundle.history.commits {
        for (index, event) in commit.events.iter().enumerate() {
            if pred(event) {
                return Some((
                    HistoryItemRef {
                        revision: bundle.revision,
                        token: format!("event:{}:{index}", commit.revision),
                    },
                    commit.revision,
                    index as u32,
                ));
            }
        }
    }
    None
}
