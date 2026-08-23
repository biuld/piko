use super::*;
use piko_protocol::messages::UpstreamAction;

/// Presentation for a provider-side ("upstream") tool card. Mirrors a normal
/// tool card: title row shows the provider tool name plus a lifecycle/approval
/// status badge; the expandable body carries the approval summary or the typed
/// action (search queries / opened URL).
pub(super) fn upstream_presentation(tool: &ToolEntry, up: &UpstreamInfo) -> ToolPresentation {
    let is_approval = up.summary.is_some();
    let (badge_text, tone) = if is_approval {
        ("!", BadgeTone::Warning)
    } else {
        match tool.status {
            ToolStatus::Running => (RUNNING_GLYPH, BadgeTone::Running),
            ToolStatus::Completed => (SUCCESS_GLYPH, BadgeTone::Success),
            ToolStatus::Failed => (FAIL_GLYPH, BadgeTone::Error),
            ToolStatus::Cancelled => (CANCELLED_GLYPH, BadgeTone::Warning),
        }
    };
    let (action_meta, action_body) = upstream_action_body(up);
    let meta = if is_approval {
        "approval".to_string()
    } else if let Some(query) = action_meta.as_deref().filter(|q| !q.is_empty()) {
        query.to_string()
    } else if !up.kind.is_empty() && up.kind != tool.name {
        up.kind.clone()
    } else {
        match tool.status {
            ToolStatus::Running => "in progress",
            ToolStatus::Completed => "completed",
            ToolStatus::Failed => "failed",
            ToolStatus::Cancelled => "cancelled",
        }
        .to_string()
    };
    let mut body_lines = Vec::new();
    if let Some(summary) = up
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        body_lines.push(BodyLine::quote(summary));
    }
    body_lines.extend(action_body);
    // `type` reflects the provider action type (`search`/`open_page`), not the
    // catalog kind, which stays `search` for every web_search action.
    let action_type = match &up.action {
        Some(UpstreamAction::Search { .. }) => Some("search"),
        Some(UpstreamAction::OpenPage { .. }) => Some("open_page"),
        None => None,
    };
    let type_label = action_type.unwrap_or(up.kind.as_str());
    if !type_label.is_empty() && type_label != tool.name {
        body_lines.push(BodyLine::meta("type", type_label));
    }
    let body = if body_lines.is_empty() {
        ToolBody::Empty
    } else {
        ToolBody::Blocks(body_lines)
    };
    ToolPresentation {
        title_meta: Some(meta),
        collapsed_preview: String::new(),
        title_badge: Some(TitleBadge::new(badge_text, tone)),
        body,
    }
}

/// Derive the title meta and body rows from the typed upstream `action`. The
/// decode boundary already strips provider-internal markers, so this is pure
/// display with no JSON parsing.
fn upstream_action_body(up: &UpstreamInfo) -> (Option<String>, Vec<BodyLine>) {
    let mut body = Vec::new();
    let meta = match &up.action {
        Some(UpstreamAction::Search { queries }) => {
            for query in queries.iter().filter(|q| !q.trim().is_empty()) {
                body.push(BodyLine::meta("query", query.as_str()));
            }
            queries.iter().find(|q| !q.trim().is_empty()).cloned()
        }
        Some(UpstreamAction::OpenPage { url }) => {
            body.push(BodyLine::meta("url", url.as_str()));
            Some(url.clone())
        }
        None => None,
    };
    (meta, body)
}
