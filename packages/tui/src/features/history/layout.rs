//! One geometry recipe shared by frame preparation and painting.
use super::HistoryPanel;
use crate::{
    app::HitId,
    ui::components::{
        pane::{PanePadding, PanePlan, prepare_pane},
        split_pane::{SplitPanePlan, SplitPaneSpec},
    },
};
use piko_tui_layout::{SplitSize, ViewportState};
use ratatui::layout::Rect;

pub(super) struct HistoryLayout {
    pub pane: PanePlan,
    pub split: SplitPanePlan,
    pub list_area: Option<Rect>,
    pub list_body: Option<Rect>,
    pub list_viewport: ViewportState,
    pub hits: Vec<(Rect, HitId)>,
    pub tabs: Vec<Rect>,
}

impl HistoryPanel {
    pub(super) fn prepare_layout(&self, area: Rect) -> Option<HistoryLayout> {
        let breadcrumb = self.breadcrumb();
        let spec = self.pane_spec(&breadcrumb);
        let pane = prepare_pane(area, &spec)?;
        let mut tabs = Vec::new();
        if !self.choosing_session && pane.content.height > 0 {
            let labels = super::render::LENS_LABELS;
            let required: u16 = labels.iter().map(|label| label.len() as u16 + 2).sum();
            let mut x = pane.content.x;
            for label in labels {
                let width = if pane.content.width >= required {
                    label.len() as u16 + 2 + ((pane.content.width - required) / 4).min(3)
                } else {
                    pane.content.width / 4
                };
                tabs.push(Rect::new(x, pane.content.y, width, 1));
                x += width;
            }
        }
        let reserved = if tabs.is_empty() {
            0
        } else {
            2.min(pane.content.height)
        };
        let content = Rect::new(
            pane.content.x,
            pane.content.y + reserved,
            pane.content.width,
            pane.content.height.saturating_sub(reserved),
        );
        let split = SplitPaneSpec {
            first: SplitSize::Percent(46),
            minimum: [34, 42],
            padding: PanePadding::new(1, 0),
            separator: 1,
        }
        .prepare(content, self.active_pane);
        let list_area = if self.choosing_session {
            Some(content)
        } else {
            split.first.map(|region| region.content)
        };
        let list_body = list_area.map(|area| {
            Rect::new(
                area.x,
                area.y.saturating_add(u16::from(area.height > 0)),
                area.width,
                area.height
                    .saturating_sub(1)
                    .saturating_sub(u16::from(self.error.is_some())),
            )
        });
        let mut viewport = self.viewport.get();
        if let Some(list) = list_body {
            viewport.set_metrics(self.row_count(), usize::from(list.height));
            viewport.ensure_visible(self.selected..self.selected.saturating_add(1));
        }
        let mut hits = tabs
            .iter()
            .enumerate()
            .map(|(index, rect)| (*rect, HitId::Mode(index)))
            .collect::<Vec<_>>();
        if let Some(list) = list_body {
            for (offset, index) in viewport.visible_range().enumerate() {
                hits.push((
                    Rect::new(
                        list.x,
                        list.y + offset as u16,
                        list.width.saturating_sub(2),
                        1,
                    ),
                    HitId::Row(index),
                ));
                if list.width >= 2 {
                    hits.push((
                        Rect::new(list.right() - 2, list.y + offset as u16, 2, 1),
                        HitId::Inspect(index),
                    ));
                }
            }
        }
        if !self.choosing_session
            && let Some(detail) = split.second
        {
            hits.push((detail.content, HitId::Content));
        }
        Some(HistoryLayout {
            pane,
            split,
            list_area,
            list_body,
            list_viewport: viewport,
            hits,
            tabs,
        })
    }
}
