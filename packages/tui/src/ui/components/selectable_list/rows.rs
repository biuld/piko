//! Line-layout paint for [`super::SelectableItem`] (Stacked / Settings / Columns).

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::{
    GroupHeaderStyle, SETTINGS_BULLET, SETTINGS_EXPAND, SelectableItem, SelectableRowLayout,
};
use crate::theme::Theme;
use crate::ui::components::feedback::{
    row_detail_style, row_primary_style, selection_prefix, with_active_text, with_selected_bg,
};
use crate::ui::components::pane::section_rule_line;

pub(super) fn row_lines(
    item: &SelectableItem,
    is_selected: bool,
    row_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    match item.layout {
        SelectableRowLayout::Stacked => stacked_lines(item, is_selected, row_width, theme),
        SelectableRowLayout::Columns => {
            // Columns normally paints through the table body; this is only a
            // defensive fallback for mixed-layout lists.
            stacked_lines(item, is_selected, row_width, theme)
        }
        SelectableRowLayout::SettingsRow => {
            vec![settings_row_line(item, is_selected, row_width, theme)]
        }
        SelectableRowLayout::SettingsOption => {
            settings_option_lines(item, is_selected, row_width, theme)
        }
    }
}

fn primary_style_for(is_selected: bool, is_active: bool, theme: &Theme) -> Style {
    let primary_style = with_selected_bg(row_primary_style(is_selected, theme), is_selected, theme);
    with_active_text(primary_style, is_selected, is_active, theme)
}

pub(super) fn leading_group_lines(
    item: &SelectableItem,
    not_first: bool,
    prev: Option<&SelectableItem>,
    row_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let this_group = item.group.as_deref();
    let prev_group = prev.and_then(|p| p.group.as_deref());
    let group_changed = this_group.is_some_and(|g| Some(g) != prev_group);
    if !group_changed {
        return lines;
    }
    let Some(header) = &item.group else {
        return lines;
    };

    match item.group_style {
        GroupHeaderStyle::Caption => {
            if not_first {
                lines.push(Line::default());
            }
            lines.push(Line::from(Span::styled(
                format!("  {header}"),
                Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        GroupHeaderStyle::Rule => {
            // One blank row between domain chunks (screenshot language).
            if not_first {
                lines.push(Line::default());
            }
            lines.push(section_rule_line(header, row_width, theme));
        }
    }
    lines
}

fn stacked_lines(
    item: &SelectableItem,
    is_selected: bool,
    row_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let primary_style = primary_style_for(is_selected, item.is_active, theme);
    let primary_disp = middle_elide_chars(&item.primary, 60, 30, 27);
    let marker = selection_prefix(is_selected);
    let marker_style = with_selected_bg(
        if is_selected {
            Style::default().fg(theme.accent)
        } else {
            Style::default()
        },
        is_selected,
        theme,
    );
    let chrome_style = with_selected_bg(Style::default().fg(theme.dim), is_selected, theme);
    let pad_style = with_selected_bg(Style::default(), is_selected, theme);
    let mut primary_spans = vec![
        Span::styled(marker, marker_style),
        Span::styled(primary_disp, primary_style),
    ];
    if let Some(trailing) = &item.trailing {
        primary_spans.push(Span::styled(format!(" {trailing}"), chrome_style));
    }
    let primary_used: usize = primary_spans
        .iter()
        .map(|s| s.content.chars().count())
        .sum();
    if primary_used < row_width {
        primary_spans.push(Span::styled(
            " ".repeat(row_width - primary_used),
            pad_style,
        ));
    }

    let badge_len = item
        .badge
        .as_ref()
        .map(|b| b.chars().count() + 3)
        .unwrap_or(0);
    let detail_budget = row_width.saturating_sub(2).saturating_sub(badge_len);
    let detail_disp = end_elide_chars(&item.detail, detail_budget);

    let detail_style = with_selected_bg(row_detail_style(theme), is_selected, theme);
    let badge_style = with_selected_bg(Style::default().fg(theme.warning), is_selected, theme);
    let mut detail_spans = vec![Span::styled(format!("  {detail_disp}"), detail_style)];
    if let Some(badge) = &item.badge {
        detail_spans.push(Span::styled(format!(" [{badge}]"), badge_style));
    }
    let detail_used: usize = detail_spans.iter().map(|s| s.content.chars().count()).sum();
    if detail_used < row_width {
        detail_spans.push(Span::styled(" ".repeat(row_width - detail_used), pad_style));
    }

    vec![Line::from(primary_spans), Line::from(detail_spans)]
}

/// Settings catalog: `▸ Label ………… value [badge] >`
fn settings_row_line(
    item: &SelectableItem,
    is_selected: bool,
    row_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let bullet = SETTINGS_BULLET;
    let expand = SETTINGS_EXPAND;
    let label_style = with_selected_bg(
        if is_selected {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        },
        is_selected,
        theme,
    );
    let value_style = with_selected_bg(row_detail_style(theme), is_selected, theme);
    let expand_style = with_selected_bg(Style::default().fg(theme.dim), is_selected, theme);
    let bullet_style = with_selected_bg(Style::default().fg(theme.dim), is_selected, theme);
    let badge_style = with_selected_bg(Style::default().fg(theme.warning), is_selected, theme);
    let pad_style = with_selected_bg(Style::default(), is_selected, theme);

    let badge_affix = item
        .badge
        .as_ref()
        .map(|b| format!(" [{b}]"))
        .unwrap_or_default();

    let left_fixed = bullet.chars().count();
    let right_fixed = expand.chars().count() + badge_affix.chars().count();
    let has_value = !item.detail.is_empty();
    let budget = row_width
        .saturating_sub(left_fixed)
        .saturating_sub(right_fixed)
        .saturating_sub(usize::from(has_value));

    let max_key = if has_value {
        (budget * 55 / 100).max(8).min(budget)
    } else {
        budget
    };
    let key_disp = end_elide_chars(&item.primary, max_key);
    let key_chars = key_disp.chars().count();
    let value_disp = end_elide_chars(&item.detail, budget.saturating_sub(key_chars));
    let value_chars = value_disp.chars().count();
    let used =
        left_fixed + key_chars + value_chars + right_fixed + usize::from(!value_disp.is_empty());
    let pad = row_width.saturating_sub(used);

    let mut spans = vec![
        Span::styled(bullet.to_string(), bullet_style),
        Span::styled(key_disp, label_style),
    ];
    if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), pad_style));
    }
    if !value_disp.is_empty() {
        spans.push(Span::styled(value_disp, value_style));
    }
    if !badge_affix.is_empty() {
        spans.push(Span::styled(badge_affix, badge_style));
    }
    spans.push(Span::styled(expand.to_string(), expand_style));
    Line::from(spans)
}

/// Settings choice: primary with Active · detail consequence under.
fn settings_option_lines(
    item: &SelectableItem,
    is_selected: bool,
    row_width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let bullet = SETTINGS_BULLET;
    let bullet_style = with_selected_bg(Style::default().fg(theme.dim), is_selected, theme);
    let label_style = with_active_text(
        with_selected_bg(
            if is_selected {
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            },
            is_selected,
            theme,
        ),
        is_selected,
        item.is_active,
        theme,
    );
    let left = bullet.chars().count();
    let budget = row_width.saturating_sub(left);
    let key = end_elide_chars(&item.primary, budget);
    let pad = row_width.saturating_sub(left + key.chars().count());

    let mut primary_spans = vec![
        Span::styled(bullet.to_string(), bullet_style),
        Span::styled(key, label_style),
    ];
    if pad > 0 {
        primary_spans.push(Span::styled(
            " ".repeat(pad),
            with_selected_bg(Style::default(), is_selected, theme),
        ));
    }

    let badge = item
        .badge
        .as_ref()
        .map(|b| format!(" [{b}]"))
        .unwrap_or_default();
    let indent = "  ";
    let detail_budget = row_width
        .saturating_sub(indent.chars().count())
        .saturating_sub(badge.chars().count());
    let detail = end_elide_chars(&item.detail, detail_budget);
    let detail_style = with_selected_bg(row_detail_style(theme), is_selected, theme);
    let badge_style = with_selected_bg(Style::default().fg(theme.warning), is_selected, theme);
    let pad_style = with_selected_bg(Style::default(), is_selected, theme);
    let mut detail_spans = vec![Span::styled(format!("{indent}{detail}"), detail_style)];
    if !badge.is_empty() {
        detail_spans.push(Span::styled(badge, badge_style));
    }
    let detail_used: usize = detail_spans.iter().map(|s| s.content.chars().count()).sum();
    if detail_used < row_width {
        detail_spans.push(Span::styled(" ".repeat(row_width - detail_used), pad_style));
    }

    vec![Line::from(primary_spans), Line::from(detail_spans)]
}

fn end_elide_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }
    let mut d = text
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    d.push_str("...");
    d
}

pub(super) fn middle_elide_chars(
    text: &str,
    max_chars: usize,
    head_chars: usize,
    tail_chars: usize,
) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let head = text.chars().take(head_chars).collect::<String>();
    let tail = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::middle_elide_chars;

    #[test]
    fn middle_elide_handles_multibyte_text() {
        let text = "这是一个包含很多中文字符的会话树条目，用来验证截断不会落在字符边界中间导致崩溃";
        let elided = middle_elide_chars(text, 20, 10, 8);

        assert!(elided.contains("..."));
        assert!(elided.starts_with("这是一个包含很多中"));
        assert!(elided.ends_with("边界中间导致崩溃"));
    }
}
