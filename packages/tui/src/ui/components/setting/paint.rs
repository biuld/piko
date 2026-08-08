//! Settings surface → [`FilterableItem`] rows drawn by FilterableList.
//!
//! Domain model stays in the setting kit; paint maps onto list layouts and a
//! product-shaped [`PaneSpec`] (title · `/ to search` · content · pipe hints).

use ratatui::{Frame, layout::Rect};

use super::{FrameContent, NavFrame};
use crate::theme::Theme;
use crate::ui::components::{
    feedback::{settings_apply_hints, settings_open_hints},
    filterable_list::{FilterableItem, render_filterable_list_with_pane},
    pane::PaneSpec,
};

pub(super) fn paint_frame<T: Clone>(
    frame: &mut Frame<'_>,
    area: Rect,
    nav: &NavFrame<T>,
    at_root: bool,
    filter: &str,
    theme: &Theme,
) {
    let title = if at_root {
        "Settings"
    } else {
        nav.title.as_str()
    };

    let (items, hints) = match &nav.content {
        FrameContent::Sections(sections) => {
            let items: Vec<FilterableItem> = sections
                .iter()
                .map(|s| {
                    let mut row = FilterableItem::new(&s.title, &s.value_summary).settings_row();
                    if let Some(badge) = s.effect.badge_label() {
                        row = row.badge(badge);
                    }
                    if let Some(g) = &s.group {
                        row = row.with_group(g.clone()).group_rule();
                    }
                    row
                })
                .collect();
            (items, settings_open_hints(at_root))
        }
        FrameContent::Choice(choice) => {
            let items: Vec<FilterableItem> = choice
                .options
                .iter()
                .map(|o| {
                    let mut row = FilterableItem::new(&o.label, &o.detail)
                        .settings_option()
                        .active(o.is_active);
                    if let Some(badge) = choice.effect.badge_label() {
                        row = row.badge(badge);
                    }
                    row
                })
                .collect();
            (items, settings_apply_hints())
        }
    };

    // Product chrome: plain title + [x], search rule, no [n/total] counter.
    let mut spec = PaneSpec::new(title)
        .search_filter(filter)
        .search_rule(true)
        .hints(hints)
        .focused(true);
    if at_root {
        spec = spec.title_right(Some("[x]"));
    }

    render_filterable_list_with_pane(frame, area, spec, &items, nav.list.selected, filter, theme);
}
