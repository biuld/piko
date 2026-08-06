use crate::api::Message;

use super::HostState;

impl HostState {
    pub fn track_turn_file_change(
        &mut self,
        session_id: &str,
        turn_id: &str,
        change: piko_protocol::TurnFileChange,
    ) -> Result<Option<piko_protocol::TurnDiffEvent>, crate::api::ProtocolError> {
        let Some(turn) = self.session_mut(session_id)?.turns.get_mut(turn_id) else {
            return Ok(None);
        };
        merge_file_change(&mut turn.file_changes, change);
        Ok(
            (!turn.file_changes.is_empty()).then(|| piko_protocol::TurnDiffEvent {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                unified_diff: render_turn_diff(&turn.file_changes),
                files: turn.file_changes.clone(),
            }),
        )
    }

    pub fn turn_diff(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Option<piko_protocol::TurnDiffEvent> {
        let turn = self.session(session_id).ok()?.turns.get(turn_id)?;
        (!turn.file_changes.is_empty()).then(|| piko_protocol::TurnDiffEvent {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            unified_diff: render_turn_diff(&turn.file_changes),
            files: turn.file_changes.clone(),
        })
    }
}

pub(crate) fn file_change_from_message(message: &Message) -> Option<piko_protocol::TurnFileChange> {
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
    changes: &mut Vec<piko_protocol::TurnFileChange>,
    change: piko_protocol::TurnFileChange,
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

pub(crate) fn render_turn_diff(changes: &[piko_protocol::TurnFileChange]) -> String {
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
