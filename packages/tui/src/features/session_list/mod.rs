use piko_protocol::{SessionListScope, SessionSummary};
mod pointer;
#[cfg(test)]
mod tests;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    widgets::Paragraph,
};

use crate::app::HitId;
use crate::theme::Theme;
use crate::ui::components::pane::{PaneAffixHit, PaneFooter, PaneSpec, PaneTitleAffix};
use crate::ui::components::selectable_list::{
    ColumnAlign, ColumnCell, SelectableItem, SelectableList, SelectablePanelBody,
    paint_selectable_panel, selectable_row_regions,
};

const SESSION_STATUS_COLUMN_WIDTH: u16 = 15;

/// Render context for the sessions surface.
pub struct SessionListCtx<'a> {
    pub active_session_id: Option<&'a str>,
    pub theme: &'a Theme,
    pub tip: Option<&'a str>,
    pub hints: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionScope {
    CurrentFolder,
    All,
}

impl SessionScope {
    pub fn to_protocol(self) -> SessionListScope {
        match self {
            SessionScope::CurrentFolder => SessionListScope::CurrentFolder,
            SessionScope::All => SessionListScope::All,
        }
    }
}

/// Resume Session panel.
pub struct SessionList {
    pub list: SelectableList<SessionSummary>,
    pub filter: String,
    pub scope: SessionScope,
    pub named_only: bool,
    pub show_path: bool,
    pub loading: bool,
    pub error: Option<String>,
}

impl SessionList {
    fn title_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let active = usize::from(self.scope == SessionScope::All);
        let sources: Vec<_> = self
            .list
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.filter_matches(item, &self.filter))
            .map(|(index, _)| index)
            .collect();
        let at = sources
            .iter()
            .position(|&index| index == self.list.selected)
            .map(|index| index + 1)
            .unwrap_or(0);
        PaneSpec::new("Resume Session")
            .title_affixes([
                PaneTitleAffix::mode_strip_static(&["Current", "All"], active),
                PaneTitleAffix::selection(at, sources.len()),
            ])
            .title_affix_regions(area)
            .into_iter()
            .filter_map(|(rect, hit)| match hit {
                PaneAffixHit::ModeOption(i) => Some((rect, HitId::Mode(i))),
                PaneAffixHit::Close => None,
            })
            .collect()
    }

    fn row_regions(&self, area: Rect) -> Vec<(Rect, usize)> {
        if self.loading || self.error.is_some() {
            return Vec::new();
        }
        let sources: Vec<usize> = self
            .list
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.filter_matches(item, &self.filter))
            .map(|(i, _)| i)
            .collect();
        let show_path_col = self.show_path || self.scope == SessionScope::All;
        let items: Vec<SelectableItem> = sources
            .iter()
            .map(|&i| session_row(&self.list.items[i], None, self.show_path, show_path_col))
            .collect();
        let selected = sources
            .iter()
            .position(|&i| i == self.list.selected)
            .unwrap_or(0);
        let scope_active = usize::from(self.scope == SessionScope::All);
        let spec = PaneSpec::new("Resume Session")
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .title_affixes([
                PaneTitleAffix::mode_strip_static(&["Current", "All"], scope_active),
                PaneTitleAffix::selection(
                    if items.is_empty() { 0 } else { selected + 1 },
                    items.len(),
                ),
            ])
            .search_filter(&self.filter)
            // Hit testing only needs the same one-row chrome budget. The
            // actual text is supplied by the render context from the binding
            // registry.
            .tip(" ")
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true);
        selectable_row_regions(area, &spec, &items, selected, "")
            .into_iter()
            .filter_map(|(rect, display)| {
                sources.get(display).copied().map(|source| (rect, source))
            })
            .collect()
    }
    pub fn new() -> Self {
        Self {
            list: SelectableList::new(Vec::new()),
            filter: String::new(),
            scope: SessionScope::CurrentFolder,
            named_only: false,
            show_path: false,
            loading: false,
            error: None,
        }
    }

    pub fn load(&mut self, mut sessions: Vec<SessionSummary>) {
        // Sort sessions by modified_at descending, then created_at, then session_id.
        sessions.sort_by(|a, b| {
            let a_mod = a.modified_at.as_deref().unwrap_or("");
            let b_mod = b.modified_at.as_deref().unwrap_or("");
            let cmp = b_mod.cmp(a_mod); // descending
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
            let a_cre = a.created_at.as_deref().unwrap_or("");
            let b_cre = b.created_at.as_deref().unwrap_or("");
            let cmp2 = b_cre.cmp(a_cre); // descending
            if cmp2 != std::cmp::Ordering::Equal {
                return cmp2;
            }
            a.session_id.cmp(&b.session_id)
        });
        self.list = SelectableList::new(sessions);
        self.loading = false;
        self.error = None;
    }

    pub fn select_next(&mut self) {
        let named_only = self.named_only;
        let show_path = self.show_path;
        let filter = self.filter.as_str();
        self.list.select_next(filter, |item| {
            if named_only && item.name.is_none() {
                return false;
            }
            Self::matches_item_static(item, filter, show_path)
        });
    }

    pub fn select_prev(&mut self) {
        let named_only = self.named_only;
        let show_path = self.show_path;
        let filter = self.filter.as_str();
        self.list.select_prev(filter, |item| {
            if named_only && item.name.is_none() {
                return false;
            }
            Self::matches_item_static(item, filter, show_path)
        });
    }

    pub fn selected_session_id(&self) -> Option<String> {
        self.selected_session_summary().map(|s| s.session_id)
    }

    pub fn selected_session_summary(&self) -> Option<SessionSummary> {
        let filter = self.filter.as_str();
        let filtered = self
            .list
            .filtered_indices(filter, |item| self.filter_matches(item, filter));
        if filtered.is_empty() {
            return None;
        }
        let selected_filtered_idx = filtered
            .iter()
            .position(|&orig_idx| orig_idx == self.list.selected)
            .unwrap_or(0)
            .min(filtered.len().saturating_sub(1));
        filtered
            .get(selected_filtered_idx)
            .and_then(|&orig_idx| self.list.items.get(orig_idx))
            .cloned()
    }

    pub fn filter_matches(&self, item: &SessionSummary, filter: &str) -> bool {
        if self.named_only && item.name.is_none() {
            return false;
        }
        Self::matches_item_static(item, filter, self.show_path)
    }

    fn matches_item_static(item: &SessionSummary, filter: &str, show_path: bool) -> bool {
        let f = filter.to_lowercase();
        if item.session_id.to_lowercase().contains(&f) {
            return true;
        }
        if item.cwd.to_lowercase().contains(&f) {
            return true;
        }
        if item
            .name
            .as_ref()
            .is_some_and(|n| n.to_lowercase().contains(&f))
        {
            return true;
        }
        if item
            .first_message
            .as_ref()
            .is_some_and(|msg| msg.to_lowercase().contains(&f))
        {
            return true;
        }
        if show_path
            && item
                .session_path
                .as_ref()
                .is_some_and(|path| path.to_lowercase().contains(&f))
        {
            return true;
        }
        false
    }

    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        active_session_id: Option<&str>,
        theme: &Theme,
        tip: Option<&str>,
        hints: Option<&str>,
    ) {
        let filter = self.filter.as_str();

        let scope_active = match self.scope {
            SessionScope::CurrentFolder => 0,
            SessionScope::All => 1,
        };

        let filtered: Vec<&SessionSummary> = if self.loading || self.error.is_some() {
            Vec::new()
        } else {
            self.list
                .items
                .iter()
                .filter(|item| self.filter_matches(item, filter))
                .collect()
        };

        let selected_filtered_idx = if filtered.is_empty() {
            0
        } else {
            self.list
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| self.filter_matches(item, filter))
                .position(|(orig_idx, _)| orig_idx == self.list.selected)
                .unwrap_or(0)
                .min(filtered.len().saturating_sub(1))
        };

        let counter_at = if filtered.is_empty() {
            0
        } else {
            selected_filtered_idx + 1
        };
        let counter_of = filtered.len();

        let show_path_col = self.show_path || self.scope == SessionScope::All;
        let mut widths = vec![Constraint::Fill(1)];
        if show_path_col {
            widths.push(Constraint::Percentage(30));
        }
        widths.extend([
            Constraint::Length(SESSION_STATUS_COLUMN_WIDTH),
            Constraint::Length(8),
        ]);

        let items: Vec<SelectableItem> = filtered
            .iter()
            .map(|item| session_row(item, active_session_id, self.show_path, show_path_col))
            .collect();

        let mut spec = PaneSpec::new("Resume Session")
            .mode(crate::ui::components::pane::PaneMode::Standard)
            .title_affixes([
                PaneTitleAffix::mode_strip_static(&["Current", "All"], scope_active),
                PaneTitleAffix::selection(counter_at, counter_of),
            ])
            .search_filter(filter)
            .tip(tip.or(Some(" ")))
            .focused(true);
        spec = match hints.filter(|value| !value.is_empty()) {
            Some(hints) => spec.hints(hints),
            None => spec.footer(PaneFooter::Reserved { height: 1 }),
        };

        let body = if self.loading {
            SelectablePanelBody::Message(
                Paragraph::new("Loading sessions...").style(Style::default().fg(theme.muted)),
            )
        } else if let Some(err) = &self.error {
            SelectablePanelBody::Message(
                Paragraph::new(format!("Error: {err}")).style(Style::default().fg(theme.error)),
            )
        } else if items.is_empty() {
            let msg = if filter.is_empty() {
                if self.scope == SessionScope::CurrentFolder {
                    "No sessions in this folder. Press Tab to view All sessions."
                } else {
                    "No sessions found."
                }
            } else {
                "No sessions match the filter."
            };
            SelectablePanelBody::Message(
                Paragraph::new(msg).style(Style::default().fg(theme.muted)),
            )
        } else {
            SelectablePanelBody::Columns {
                items: &items,
                selected: selected_filtered_idx,
                widths: Some(&widths),
            }
        };

        let _ = paint_selectable_panel(frame, area, theme, &spec, body);
    }
}

fn session_row(
    item: &SessionSummary,
    active_session_id: Option<&str>,
    show_path: bool,
    show_path_col: bool,
) -> SelectableItem {
    let mut title_text = if let Some(n) = &item.name {
        n.clone()
    } else if let Some(msg) = &item.first_message {
        let cleaned: String = msg
            .chars()
            .map(|c| {
                if c == '\n' || c == '\r' || c == '\t' {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let char_count = cleaned.chars().count();
        if char_count > 40 {
            let truncated: String = cleaned.chars().take(37).collect();
            format!("{truncated}...")
        } else {
            cleaned
        }
    } else {
        "untitled".to_string()
    };
    if item.integrity_error.is_some() {
        title_text = format!("⚠ {title_text}");
    }

    let title_cell = if item.name.is_some() {
        ColumnCell::emphasized(title_text)
    } else {
        ColumnCell::primary(title_text)
    };

    let mut cells = vec![title_cell];

    if show_path_col {
        let path_str = if show_path && item.session_path.is_some() {
            item.session_path.clone().unwrap()
        } else {
            item.cwd.clone()
        };
        cells.push(ColumnCell::secondary(path_str));
    }

    let count = item.message_count;
    let count_str = if item.integrity_error.is_some() {
        "integrity error".to_string()
    } else if count == 1 {
        "1 message".to_string()
    } else {
        format!("{count} messages")
    };
    cells.push(ColumnCell::secondary(count_str).align(ColumnAlign::Right));
    cells.push(
        ColumnCell::secondary(format_age(item.modified_at.as_deref())).align(ColumnAlign::Right),
    );

    let is_active = active_session_id
        .map(|id| id == item.session_id)
        .unwrap_or(false);

    SelectableItem::columns(cells).active(is_active)
}

fn format_age(timestamp_str: Option<&str>) -> String {
    let Some(t_str) = timestamp_str else {
        return String::new();
    };
    let Ok(ms) = t_str.parse::<u64>() else {
        return String::new();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let diff_secs = now_ms.saturating_sub(ms) / 1000;
    if diff_secs < 60 {
        "just now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h", diff_secs / 3600)
    } else {
        format!("{}d", diff_secs / 86400)
    }
}
