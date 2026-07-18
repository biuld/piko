//! Pure view-model types derived from `ClientState`.
//!
//! Spinner ownership (design §11.1):
//! - Host connect/reconnect → StatusBar connection item (transport state)
//! - Session open/create → pending Session row (correlated command + reconcile)
//! - Initial hydrate → Timeline skeleton (SessionReconciled or SessionCleared)
//! - Agent running → Agent Tree row + Activity item (reconciled Agent activity)

use piko_client_core::state::ConnectionState;
use piko_client_core::{ClientState, SessionPhase};

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

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub session_id: String,
    pub label: String,
    pub kind: SessionRowKind,
    pub message_count: u64,
    #[allow(dead_code)] // reserved for future sidebar metadata chrome
    pub modified_at: Option<String>,
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
    pub groups: Vec<SidebarGroup>,
}

struct GroupBucket {
    display_cwd: String,
    rows: Vec<SessionRow>,
}

const PENDING_GROUP_KEY: &str = "";

/// Derive sidebar groups from the global session list + live/pending state.
///
/// Groups by working directory and sorts groups alphabetically by path.
/// The GUI session list is global (`SessionListScope::All`); there is no
/// privileged "current folder" group.
pub fn derive_sidebar(state: &ClientState) -> SidebarViewModel {
    let list = &state.session_list;
    let live_id = state.live_session.as_ref().map(|s| s.session_id.as_str());
    let pending_id = pending_target_id(&state.session_phase);

    let mut by_cwd: std::collections::HashMap<String, GroupBucket> =
        std::collections::HashMap::new();

    for s in &list.sessions {
        let kind = if live_id == Some(s.session_id.as_str()) {
            SessionRowKind::LiveTarget
        } else if pending_id == Some(s.session_id.as_str()) {
            SessionRowKind::PendingTarget
        } else {
            SessionRowKind::Listed
        };
        let row = SessionRow {
            session_id: s.session_id.clone(),
            label: session_label(s),
            kind,
            message_count: s.message_count,
            modified_at: s.modified_at.clone(),
        };
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

    // Pending target missing from the list gets a temporary Opening group.
    if let Some(pid) = pending_id {
        let already = by_cwd
            .values()
            .any(|bucket| bucket.rows.iter().any(|r| r.session_id == pid));
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
                        modified_at: None,
                    },
                );
        }
    }

    let mut keys: Vec<String> = by_cwd.keys().cloned().collect();
    keys.sort_by(|a, b| {
        // Keep the transient Opening group first; otherwise sort by path.
        match (
            a.as_str() == PENDING_GROUP_KEY,
            b.as_str() == PENDING_GROUP_KEY,
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b)),
        }
    });

    let groups = keys
        .into_iter()
        .filter_map(|key| {
            let bucket = by_cwd.remove(&key)?;
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

    SidebarViewModel { groups }
}

fn cwd_key(cwd: &str) -> String {
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

fn session_label(s: &piko_protocol::SessionSummary) -> String {
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
