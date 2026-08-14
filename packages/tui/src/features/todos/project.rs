//! Pure strip projection: TodoList → header / rows / overflow (no ratatui).

use piko_protocol::{TodoList, TodoStatus};
use unicode_width::UnicodeWidthStr;

use crate::features::dock_stack::TODOS_MAX_ITEM_ROWS;
use crate::ui::components::feedback::{DISCLOSURE_COLLAPSED, SUCCESS_GLYPH};

/// Projected strip content for one paint frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodoStripView {
    pub header: String,
    pub rows: Vec<TodoStripRow>,
    pub overflow: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TodoStripRow {
    pub mark: &'static str,
    pub content: String,
    pub status: TodoStatus,
}

/// Preferred dock height: content rows plus one Dock Stack separator row.
pub fn strip_height_offer(list: &TodoList) -> u16 {
    let n = list.items.len() as u16;
    if n == 0 {
        return 0;
    }
    let max_items = TODOS_MAX_ITEM_ROWS;
    let shown = n.min(max_items);
    let overflow = u16::from(n > max_items);
    1 + shown + overflow + 1
}

/// Project list into strip rows, truncating content to `width` and capping items.
pub fn project_strip(list: &TodoList, width: u16, max_item_rows: usize) -> TodoStripView {
    let total = list.items.len();
    let mut done = 0usize;
    let mut active = 0usize;
    let mut pending = 0usize;
    for item in &list.items {
        match item.status {
            TodoStatus::Completed => done += 1,
            TodoStatus::InProgress => active += 1,
            TodoStatus::Pending => pending += 1,
        }
    }
    let remaining = active + pending;
    let header = format!("Todos  {done}/{total} done · {active} active · {remaining} remaining");
    let header = truncate_line(&header, width);

    let max_item_rows = max_item_rows
        .min(TODOS_MAX_ITEM_ROWS as usize)
        .max(if total > 0 { 1 } else { 0 }.min(total));
    let shown = total.min(max_item_rows);
    let rows: Vec<TodoStripRow> = list
        .items
        .iter()
        .take(shown)
        .map(|item| {
            let mark = status_mark(item.status);
            // "mark" + space + content
            let content_budget = width.saturating_sub((mark.width() as u16).saturating_add(1));
            TodoStripRow {
                mark,
                content: truncate_line(&item.content, content_budget),
                status: item.status,
            }
        })
        .collect();

    let overflow = if total > shown {
        Some(format!("+{} more", total - shown))
    } else {
        None
    };

    TodoStripView {
        header,
        rows,
        overflow,
    }
}

/// Max item rows that fit inside a granted height (header + optional overflow).
pub fn max_item_rows_for_grant(grant_height: u16, item_count: usize) -> usize {
    if grant_height == 0 || item_count == 0 {
        return 0;
    }
    // Reserve 1 for header; if items exceed remaining, reserve 1 for overflow.
    let after_header = grant_height.saturating_sub(1);
    if after_header == 0 {
        return 0;
    }
    let cap = TODOS_MAX_ITEM_ROWS as usize;
    if item_count as u16 <= after_header {
        return (item_count).min(cap);
    }
    // Need overflow row when not showing all.
    let for_items = after_header.saturating_sub(1).max(1);
    (for_items as usize).min(cap).min(item_count)
}

fn status_mark(status: TodoStatus) -> &'static str {
    match status {
        TodoStatus::Completed => SUCCESS_GLYPH,
        TodoStatus::InProgress => DISCLOSURE_COLLAPSED,
        TodoStatus::Pending => "·",
    }
}

fn truncate_line(text: &str, width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    let max = width as usize;
    if text.width() <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]));
        if used + w + 1 > max {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}
