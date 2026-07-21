//! Pure view-model types derived from `ClientState`.
//!
//! Spinner ownership (design §11.1):
//! - Host connect/reconnect → StatusBar connection item (transport state)
//! - Session open/create → pending Session row (correlated command + reconcile)
//! - Initial hydrate → Timeline skeleton (SessionReconciled or SessionCleared)
//! - Agent running → Agent Tree row + Activity item (reconciled Agent activity)

use piko_client_core::state::ConnectionState;
use piko_client_core::{ClientState, SessionPhase};
use piko_protocol::SessionSummary;
use std::collections::{HashMap, HashSet};

// ─── Session phase view ──────────────────────────────────────────────────────

/// High-level phase for chrome/workbench layout decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionPhaseView {
    /// No session active, show welcome / session picker.
    IdleNoSession,
    /// An open or create is in flight (identity not yet confirmed).
    Opening { target_id: Option<String> },
    /// Identity confirmed, waiting for reconcile.
    Hydrating { target_id: String },
    /// Fully reconciled and live.
    Live,
    /// An error occurred (last_error available).
    Error { message: String },
}

/// Derive the phase view from core state.
pub fn derive_phase_view(state: &ClientState) -> SessionPhaseView {
    if let Some(err) = &state.last_error
        && !state.is_live()
    {
        return SessionPhaseView::Error {
            message: err.clone(),
        };
    }

    match &state.session_phase {
        SessionPhase::IdleNoSession => SessionPhaseView::IdleNoSession,
        SessionPhase::OpeningOrCreating { target_id } => SessionPhaseView::Opening {
            target_id: target_id.clone(),
        },
        SessionPhase::Hydrating { target_id } => SessionPhaseView::Hydrating {
            target_id: target_id.clone(),
        },
        SessionPhase::Live => SessionPhaseView::Live,
    }
}

// ─── Session sidebar ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionRowKind {
    Listed,
    LiveTarget,
    PendingTarget,
}

#[derive(Debug, Clone, Default)]
pub struct SidebarPrefs {
    pub pinned_session_ids: HashSet<String>,
    pub session_last_used_at_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub session_id: String,
    pub label: String,
    pub kind: SessionRowKind,
    pub message_count: u64,
    pub is_pinned: bool,
    /// Folder leaf shown in the global Pinned band (`detail` slot).
    pub cwd_hint: String,
}

/// Sessions for one working directory.
#[derive(Debug, Clone)]
pub struct SidebarGroup {
    pub cwd: String,
    /// Section title: folder leaf name (or abbreviated path).
    pub label: String,
    pub rows: Vec<SessionRow>,
}

#[derive(Debug, Clone)]
pub struct SidebarViewModel {
    pub pinned: Vec<SessionRow>,
    pub groups: Vec<SidebarGroup>,
}

struct GroupBucket {
    display_cwd: String,
    rows: Vec<SessionRow>,
}

const PENDING_GROUP_KEY: &str = "";

/// Derive sidebar from session list, GUI pin/MRU prefs, and live/pending state.
pub fn derive_sidebar(state: &ClientState, prefs: &SidebarPrefs) -> SidebarViewModel {
    let list = &state.session_list;
    // Only mark LiveTarget when phase is Live. During open/hydrate the previous
    // `live_session` is still present until reconcile; marking it selected while
    // the pending target is also emphasized produces a dual-highlight flash.
    let live_id = match &state.session_phase {
        SessionPhase::Live => state.live_session.as_ref().map(|s| s.session_id.as_str()),
        _ => None,
    };
    let pending_id = pending_target_id(&state.session_phase);

    let summary_by_id: HashMap<&str, &SessionSummary> = list
        .sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();

    let mut by_cwd: HashMap<String, GroupBucket> = HashMap::new();
    let mut pinned_rows: Vec<SessionRow> = Vec::new();

    for s in &list.sessions {
        let is_pinned = prefs.pinned_session_ids.contains(&s.session_id);
        let row = make_row(s, live_id, pending_id, is_pinned);
        if is_pinned && row.kind != SessionRowKind::PendingTarget {
            pinned_rows.push(row);
            continue;
        }
        let key = cwd_key(&s.cwd);
        by_cwd
            .entry(key)
            .or_insert_with(|| GroupBucket {
                display_cwd: s.cwd.clone(),
                rows: Vec::new(),
            })
            .rows
            .push(row);
    }

    if let Some(pid) = pending_id {
        let already = by_cwd
            .values()
            .any(|bucket| bucket.rows.iter().any(|r| r.session_id == pid))
            || pinned_rows.iter().any(|r| r.session_id == pid);
        if !already {
            by_cwd
                .entry(PENDING_GROUP_KEY.to_string())
                .or_insert_with(|| GroupBucket {
                    display_cwd: String::new(),
                    rows: Vec::new(),
                })
                .rows
                .insert(
                    0,
                    SessionRow {
                        session_id: pid.to_string(),
                        label: format!("Opening {}…", &pid[..pid.len().min(8)]),
                        kind: SessionRowKind::PendingTarget,
                        message_count: 0,
                        is_pinned: false,
                        cwd_hint: String::new(),
                    },
                );
        }
    }

    sort_rows_by_mru(&mut pinned_rows, &summary_by_id, prefs);
    for bucket in by_cwd.values_mut() {
        sort_rows_by_mru(&mut bucket.rows, &summary_by_id, prefs);
    }

    let mut group_entries: Vec<(String, GroupBucket)> = by_cwd.into_iter().collect();
    group_entries.sort_by(|(ka, a), (kb, b)| {
        match (
            ka.as_str() == PENDING_GROUP_KEY,
            kb.as_str() == PENDING_GROUP_KEY,
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let ra = group_mru_rank(a, &summary_by_id, prefs);
                let rb = group_mru_rank(b, &summary_by_id, prefs);
                rb.cmp(&ra).then_with(|| ka.cmp(kb))
            }
        }
    });

    let groups = group_entries
        .into_iter()
        .filter_map(|(key, bucket)| {
            if bucket.rows.is_empty() {
                return None;
            }
            let label = if key == PENDING_GROUP_KEY {
                "Opening…".to_string()
            } else {
                folder_group_label(&bucket.display_cwd)
            };
            Some(SidebarGroup {
                cwd: bucket.display_cwd,
                label,
                rows: bucket.rows,
            })
        })
        .collect();

    SidebarViewModel {
        pinned: pinned_rows,
        groups,
    }
}

fn make_row(
    s: &SessionSummary,
    live_id: Option<&str>,
    pending_id: Option<&str>,
    is_pinned: bool,
) -> SessionRow {
    let kind = if live_id == Some(s.session_id.as_str()) {
        SessionRowKind::LiveTarget
    } else if pending_id == Some(s.session_id.as_str()) {
        SessionRowKind::PendingTarget
    } else {
        SessionRowKind::Listed
    };
    SessionRow {
        session_id: s.session_id.clone(),
        label: session_label(s),
        kind,
        message_count: s.message_count,
        is_pinned,
        cwd_hint: folder_group_label(&s.cwd),
    }
}

fn effective_timestamp(
    session_id: &str,
    summary: Option<&SessionSummary>,
    prefs: &SidebarPrefs,
) -> u64 {
    if let Some(ms) = prefs.session_last_used_at_ms.get(session_id) {
        return *ms;
    }
    if let Some(s) = summary {
        if let Some(ms) = parse_timestamp_ms(s.modified_at.as_deref()) {
            return ms;
        }
        if let Some(ms) = parse_timestamp_ms(s.created_at.as_deref()) {
            return ms;
        }
    }
    0
}

fn parse_timestamp_ms(value: Option<&str>) -> Option<u64> {
    let s = value?;
    if let Ok(ms) = s.parse::<u64>() {
        return Some(ms);
    }
    // ISO-8601 strings sort lexicographically; use as pseudo-ms for ordering.
    Some(simple_string_rank(s))
}

fn simple_string_rank(s: &str) -> u64 {
    s.bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
}

fn sort_rows_by_mru(
    rows: &mut [SessionRow],
    summaries: &HashMap<&str, &SessionSummary>,
    prefs: &SidebarPrefs,
) {
    rows.sort_by(|a, b| {
        let ta = effective_timestamp(
            &a.session_id,
            summaries.get(a.session_id.as_str()).copied(),
            prefs,
        );
        let tb = effective_timestamp(
            &b.session_id,
            summaries.get(b.session_id.as_str()).copied(),
            prefs,
        );
        tb.cmp(&ta).then_with(|| a.session_id.cmp(&b.session_id))
    });
}

fn group_mru_rank(
    bucket: &GroupBucket,
    summaries: &HashMap<&str, &SessionSummary>,
    prefs: &SidebarPrefs,
) -> u64 {
    bucket
        .rows
        .iter()
        .map(|r| {
            effective_timestamp(
                &r.session_id,
                summaries.get(r.session_id.as_str()).copied(),
                prefs,
            )
        })
        .max()
        .unwrap_or(0)
}

fn cwd_key(cwd: &str) -> String {
    normalize_cwd_key(cwd)
}

/// Normalize a working-directory path for sidebar grouping and equality checks.
pub fn normalize_cwd_key(cwd: &str) -> String {
    let trimmed = cwd.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

fn folder_group_label(cwd: &str) -> String {
    let path = std::path::Path::new(cwd);
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| abbreviate_cwd(cwd))
}

fn session_label(s: &SessionSummary) -> String {
    if let Some(name) = &s.name {
        return name.clone();
    }
    if let Some(first) = &s.first_message {
        let truncated: String = first.chars().take(40).collect();
        if truncated.len() < first.len() {
            return format!("{truncated}…");
        }
        return truncated;
    }
    s.session_id[..s.session_id.len().min(8)].to_string()
}

fn pending_target_id(phase: &SessionPhase) -> Option<&str> {
    match phase {
        SessionPhase::OpeningOrCreating {
            target_id: Some(id),
        } => Some(id.as_str()),
        SessionPhase::Hydrating { target_id } => Some(target_id.as_str()),
        _ => None,
    }
}

// ─── Status bar ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct StatusBarViewModel {
    pub connection: ConnectionStatus,
    /// Show cwd when sidebar is hidden and a session is live.
    pub cwd: Option<String>,
    /// Formatted context/cost string when available.
    pub usage: Option<String>,
}

/// Derive status bar from core state + layout hint.
///
/// `session_sidebar_visible`: when true, cwd is omitted (sidebar owns path context).
pub fn derive_status_bar(state: &ClientState, session_sidebar_visible: bool) -> StatusBarViewModel {
    let connection = match state.shell.connection {
        ConnectionState::Connected => ConnectionStatus::Connected,
        ConnectionState::Disconnected => ConnectionStatus::Disconnected,
    };

    let cwd = if session_sidebar_visible {
        None
    } else {
        state.live_session.as_ref().map(|s| abbreviate_cwd(&s.cwd))
    };

    let usage = state
        .live_session
        .as_ref()
        .and_then(|s| s.cumulative_usage.as_ref())
        .map(format_usage)
        .filter(|s| !s.is_empty());

    StatusBarViewModel {
        connection,
        cwd,
        usage,
    }
}

fn abbreviate_cwd(cwd: &str) -> String {
    let path = std::path::Path::new(cwd);
    let parts: Vec<_> = path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect();
    match parts.as_slice() {
        [] => cwd.to_string(),
        [one] => one.to_string(),
        [.., a, b] => format!("…/{a}/{b}"),
    }
}

fn format_usage(usage: &piko_protocol::messages::Usage) -> String {
    let mut parts = Vec::new();
    if usage.total_tokens > 0 {
        parts.push(format!("{}tok", usage.total_tokens));
    }
    if usage.cost.total > 0.0 {
        parts.push(format!("${:.4}", usage.cost.total));
    }
    parts.join(" · ")
}
