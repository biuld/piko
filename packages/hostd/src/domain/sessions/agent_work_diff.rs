use crate::api::{Message, SessionTreeEntry};

use super::HostState;

impl HostState {
    /// Rebuild the exact-content diff for one root AgentInput from the session
    /// tree (the same facts the journal already stores on tool results).
    pub fn agent_work_diff(
        &self,
        session_id: &str,
        root_input_id: &str,
    ) -> Option<piko_protocol::AgentWorkDiffEvent> {
        let session = self.session(session_id).ok()?;
        let mut changes = Vec::new();
        for entry in &session.entries {
            let SessionTreeEntry::Message(message) = entry else {
                continue;
            };
            if message.root_input_id != root_input_id {
                continue;
            }
            if let Some(change) = file_change_from_message(&message.message) {
                merge_file_change(&mut changes, change);
            }
        }
        (!changes.is_empty()).then(|| piko_protocol::AgentWorkDiffEvent {
            session_id: session_id.to_string(),
            root_input_id: root_input_id.to_string(),
            unified_diff: render_agent_work_diff(&changes),
            files: changes,
        })
    }
}

pub(crate) fn file_change_from_message(
    message: &Message,
) -> Option<piko_protocol::AgentWorkFileChange> {
    let Message::ToolResult {
        details: Some(details),
        is_error,
        ..
    } = message
    else {
        return None;
    };
    if is_error.unwrap_or(false) {
        return None;
    }
    serde_json::from_value(details.get("_pikoFileChange")?.clone()).ok()
}

pub(crate) fn merge_file_change(
    changes: &mut Vec<piko_protocol::AgentWorkFileChange>,
    change: piko_protocol::AgentWorkFileChange,
) {
    if let Some(existing) = changes
        .iter_mut()
        .find(|existing| existing.path == change.path)
    {
        existing.after = change.after;
    } else {
        changes.push(change);
        changes.sort_by(|left, right| left.path.cmp(&right.path));
    }
    changes.retain(|change| change.before != change.after);
}

pub(crate) fn render_agent_work_diff(changes: &[piko_protocol::AgentWorkFileChange]) -> String {
    let mut rendered = String::new();
    for change in changes {
        let before_path = change
            .before
            .as_ref()
            .map(|_| format!("a/{}", change.path))
            .unwrap_or_else(|| "/dev/null".into());
        let after_path = change
            .after
            .as_ref()
            .map(|_| format!("b/{}", change.path))
            .unwrap_or_else(|| "/dev/null".into());
        rendered.push_str(&format!("--- {before_path}\n+++ {after_path}\n@@\n"));
        if let Some(before) = &change.before {
            render_prefixed_content(&mut rendered, '-', before);
        }
        if let Some(after) = &change.after {
            render_prefixed_content(&mut rendered, '+', after);
        }
    }
    rendered
}

fn render_prefixed_content(rendered: &mut String, prefix: char, content: &str) {
    for line in content.split_inclusive('\n') {
        rendered.push(prefix);
        rendered.push_str(line);
    }
    if !content.is_empty() && !content.ends_with('\n') {
        rendered.push_str("\n\\ No newline at end of file\n");
    }
}
