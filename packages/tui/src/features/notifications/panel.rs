use piko_tui_layout::{Component, InteractionState, SurfacePanel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{
    NoticePolicy, NoticeScope, NoticeStatus, Notification, NotificationCenter, NotificationLevel,
    NotificationViewScope,
};
use crate::{
    app::HitId,
    navigation::SurfaceId,
    theme::Theme,
    ui::{
        components::pane::{PaneAffixHit, PaneSpec, PaneTitleAffix, render_pane},
        interaction::paint_element_hover,
    },
};

pub struct NotificationPanelCtx<'a> {
    pub session_id: Option<&'a str>,
    pub now: Instant,
    pub theme: &'a Theme,
}

impl NotificationCenter {
    fn modal_spec(&self) -> PaneSpec<'static> {
        PaneSpec::new("Notifications")
            .title_affixes([PaneTitleAffix::mode_strip_static(
                &["Current", "All"],
                usize::from(self.view_scope == NotificationViewScope::All),
            )])
            .hints("Tab scope · ↑/↓ scroll · Esc close")
            .focused(true)
    }

    fn title_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        self.modal_spec()
            .title_affix_regions(area)
            .into_iter()
            .filter_map(|(rect, hit)| match hit {
                PaneAffixHit::ModeOption(index) => Some((rect, HitId::Mode(index))),
                PaneAffixHit::Close => None,
            })
            .collect()
    }
}

impl Component<HitId, NotificationPanelCtx<'_>> for NotificationCenter {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &NotificationPanelCtx<'_>) {
        let spec = self.modal_spec();
        let Some(areas) = render_pane(frame, area, &spec, ctx.theme) else {
            return;
        };
        let items = self.modal_items(ctx.session_id);
        if items.is_empty() {
            frame.render_widget(
                Paragraph::new("No notices in this scope.")
                    .style(Style::default().fg(ctx.theme.muted)),
                areas.content,
            );
            return;
        }
        let lines = items
            .into_iter()
            .map(|notice| notification_line(notice, ctx.now, ctx.theme))
            .collect::<Vec<_>>();
        let max_scroll = lines
            .len()
            .saturating_sub(usize::from(areas.content.height));
        frame.render_widget(
            Paragraph::new(lines).scroll((self.scroll.min(max_scroll) as u16, 0)),
            areas.content,
        );
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &NotificationPanelCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        self.render(frame, area, ctx);
        paint_element_hover(
            frame,
            &self.title_regions(area),
            interaction,
            Some(HitId::Mode(usize::from(
                self.view_scope == NotificationViewScope::All,
            ))),
            ctx.theme,
        );
    }

    fn component_regions(&self, area: Rect) -> Vec<(Rect, HitId)> {
        let mut regions = self.title_regions(area);
        if let Some(content) = self.modal_spec().content_rect(area) {
            regions.push((content, HitId::Content));
        }
        regions
    }
}

impl SurfacePanel<SurfaceId, HitId, NotificationPanelCtx<'_>> for NotificationCenter {
    fn region(&self) -> SurfaceId {
        SurfaceId::Notifications
    }
}

fn notification_line(notification: &Notification, now: Instant, theme: &Theme) -> Line<'static> {
    let (level, color) = match notification.level {
        NotificationLevel::Info => ("info", theme.info),
        NotificationLevel::Warning => ("warning", theme.warning),
        NotificationLevel::Error => ("error", theme.error),
    };
    let scope = match &notification.scope {
        NoticeScope::Global => "global".to_string(),
        NoticeScope::Session(session_id) => {
            format!("session:{}", session_id.chars().take(8).collect::<String>())
        }
    };
    let policy = match notification.policy {
        NoticePolicy::Transient { .. } => "transient",
        NoticePolicy::Dismissible => "dismissible",
        NoticePolicy::UntilResolved(_) => "pending",
    };
    let status = match notification.status {
        NoticeStatus::Active if matches!(notification.policy, NoticePolicy::Transient { visible_until } if visible_until <= now) => {
            "elapsed"
        }
        NoticeStatus::Active => "active",
        NoticeStatus::Dismissed => "dismissed",
        NoticeStatus::Resolved => "resolved",
    };
    Line::from(vec![
        Span::styled(format!("● {level:<7}"), Style::default().fg(color)),
        Span::styled(format!(" {scope:<17}"), Style::default().fg(theme.dim)),
        Span::styled(format!(" {policy:<11}"), Style::default().fg(theme.muted)),
        Span::styled(format!(" {status:<10}"), Style::default().fg(theme.dim)),
        Span::styled(
            notification.message.clone(),
            Style::default().fg(theme.text),
        ),
    ])
}
use std::time::Instant;
