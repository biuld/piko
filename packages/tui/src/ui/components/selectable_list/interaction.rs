use piko_tui_layout::InteractionState;
use ratatui::{Frame, layout::Rect, style::Style};

use super::{SelectableItem, SelectableRowLayout, item_matches_filter};
use crate::{
    app::HitId,
    theme::Theme,
    ui::components::pane::{PaneSpec, PaneTitleAffix},
};

pub fn minimal_row_regions(
    area: Rect,
    title: &str,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
) -> Vec<(Rect, usize)> {
    let filtered_count = items
        .iter()
        .filter(|item| item_matches_filter(item, filter))
        .count();
    let selected_one = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item_matches_filter(item, filter))
        .position(|(orig, _)| orig == selected)
        .map(|i| i + 1)
        .unwrap_or(0);
    let spec = PaneSpec::minimal(title)
        .affix(PaneTitleAffix::selection(selected_one, filtered_count))
        .search_filter(filter)
        .focused(true);
    selectable_row_regions(area, &spec, items, selected, filter)
}

pub fn selectable_row_regions(
    area: Rect,
    spec: &PaneSpec<'_>,
    items: &[SelectableItem],
    selected: usize,
    filter: &str,
) -> Vec<(Rect, usize)> {
    let Some(content) = spec.content_rect(area) else {
        return Vec::new();
    };
    let filtered: Vec<(usize, &SelectableItem)> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item_matches_filter(item, filter))
        .collect();
    if filtered.is_empty() {
        return Vec::new();
    }
    let selected_filtered = filtered
        .iter()
        .position(|(source, _)| *source == selected)
        .unwrap_or(0);
    let columns = filtered
        .first()
        .is_some_and(|(_, item)| item.layout == SelectableRowLayout::Columns);
    let heights: Vec<u16> = filtered
        .iter()
        .enumerate()
        .map(|(position, (_, item))| {
            if columns {
                1
            } else {
                leading_group_height(item, position, &filtered)
                    .saturating_add(item_row_height(item))
            }
        })
        .collect();
    let (first, last) = visible_bounds(&heights, selected_filtered, content.height);
    let mut y = content.y;
    let mut out = Vec::new();
    for position in first..last {
        let (source, item) = filtered[position];
        let leading = if columns {
            0
        } else {
            leading_group_height(item, position, &filtered)
        };
        y = y.saturating_add(leading);
        let row_height = if columns { 1 } else { item_row_height(item) };
        let visible = row_height.min(content.y.saturating_add(content.height).saturating_sub(y));
        if visible > 0 {
            out.push((Rect::new(content.x, y, content.width, visible), source));
        }
        y = y.saturating_add(row_height);
    }
    out
}

pub fn paint_row_hover(
    frame: &mut Frame<'_>,
    regions: &[(Rect, usize)],
    interaction: InteractionState<HitId>,
    selected: usize,
    theme: &Theme,
) {
    let index = match interaction.hovered {
        Some(HitId::Row(index)) => Some(index),
        _ => None,
    };
    paint_index_hover(frame, regions, index, selected, theme);
}

/// Paint hover feedback for a selectable row whose owner uses a custom hit id.
pub fn paint_index_hover(
    frame: &mut Frame<'_>,
    regions: &[(Rect, usize)],
    hovered: Option<usize>,
    selected: usize,
    theme: &Theme,
) {
    let Some(index) = hovered else { return };
    if index == selected {
        return;
    }
    let Some(background) = crate::ui::components::hover_bg(theme) else {
        return;
    };
    if let Some((rect, _)) = regions.iter().find(|(_, source)| *source == index) {
        frame
            .buffer_mut()
            .set_style(*rect, Style::default().bg(background));
    }
}

fn item_row_height(item: &SelectableItem) -> u16 {
    match item.layout {
        SelectableRowLayout::Columns | SelectableRowLayout::SettingsRow => 1,
        SelectableRowLayout::Stacked | SelectableRowLayout::SettingsOption => 2,
    }
}

fn leading_group_height(
    item: &SelectableItem,
    position: usize,
    filtered: &[(usize, &SelectableItem)],
) -> u16 {
    let previous = position
        .checked_sub(1)
        .and_then(|i| filtered.get(i))
        .and_then(|(_, row)| row.group.as_deref());
    if item
        .group
        .as_deref()
        .is_none_or(|group| Some(group) == previous)
    {
        return 0;
    }
    1 + u16::from(position > 0)
}

fn visible_bounds(heights: &[u16], selected: usize, max_height: u16) -> (usize, usize) {
    if heights.is_empty() || max_height == 0 {
        return (0, 0);
    }
    let mut first = 0;
    let mut last = 0;
    let mut height = 0u16;
    while last < heights.len() && height.saturating_add(heights[last]) <= max_height {
        height = height.saturating_add(heights[last]);
        last += 1;
    }
    let target = selected.min(heights.len() - 1);
    while target >= last && last < heights.len() {
        height = height.saturating_add(heights[last]);
        last += 1;
        while height > max_height && first < last {
            height = height.saturating_sub(heights[first]);
            first += 1;
        }
    }
    (first, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtered_regions_keep_source_indices() {
        let items = vec![
            SelectableItem::columns([super::super::ColumnCell::primary("hide")]),
            SelectableItem::columns([super::super::ColumnCell::primary("keep one")]),
            SelectableItem::columns([super::super::ColumnCell::primary("keep two")]),
        ];
        let spec = PaneSpec::minimal("test").search_filter("keep").hints("Esc");
        let regions = selectable_row_regions(Rect::new(0, 0, 40, 10), &spec, &items, 1, "keep");
        assert_eq!(
            regions.iter().map(|(_, index)| *index).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn selected_row_is_kept_in_small_viewport() {
        let items: Vec<_> = (0..8)
            .map(|i| SelectableItem::columns([super::super::ColumnCell::primary(i.to_string())]))
            .collect();
        let spec = PaneSpec::minimal("test").no_search();
        let regions = selectable_row_regions(Rect::new(0, 0, 20, 4), &spec, &items, 7, "");
        assert!(regions.iter().any(|(_, index)| *index == 7));
    }
}
