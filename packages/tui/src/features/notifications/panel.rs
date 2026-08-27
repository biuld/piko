use std::time::Instant;

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
    NotificationViewScope, level_glyph,
};
use crate::{
    app::HitId,
    navigation::SurfaceId,
    theme::Theme,
    ui::{
        components::pane::{PaneAffixHit, PaneFooter, PaneSpec, PaneTitleAffix, render_pane},
        interaction::paint_element_hover,
        line_layout::{paint_cols, soft_wrap, truncate_paint_cols},
    },
};

pub struct NotificationPanelCtx<'a> {
    pub session_id: Option<&'a str>,
    pub now: Instant,
    pub theme: &'a Theme,
    pub hints: Option<&'a str>,
}

impl NotificationCenter {
    fn modal_spec(&self) -> PaneSpec<'static> {
        PaneSpec::new("Notifications")
            .title_affixes([PaneTitleAffix::mode_strip_static(
                &["Current", "All"],
                usize::from(self.view_scope == NotificationViewScope::All),
            )])
            .footer(PaneFooter::Reserved { height: 1 })
            .focused(true)
    }

    fn modal_spec_with_hints<'a>(&self, hints: Option<&'a str>) -> PaneSpec<'a> {
        let spec = PaneSpec::new("Notifications")
            .title_affixes([PaneTitleAffix::mode_strip_static(
                &["Current", "All"],
                usize::from(self.view_scope == NotificationViewScope::All),
            )])
            .focused(true);
        if let Some(hints) = hints.filter(|value| !value.is_empty()) {
            spec.hints(hints)
        } else {
            spec.footer(PaneFooter::Reserved { height: 1 })
        }
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

    pub(crate) fn copy_regions(&self, area: Rect, session_id: Option<&str>) -> Vec<(Rect, HitId)> {
        let Some(content) = self.modal_spec().content_rect(area) else {
            return Vec::new();
        };
        let button_width = COPY_LABEL_WIDTH.min(content.width);
        let scroll = self.scroll.min(self.max_scroll.get());
        let mut row = 0usize;
        self.modal_items(session_id)
            .into_iter()
            .filter_map(|notice| {
                let header_row = row;
                row = row.saturating_add(notification_height(notice, content.width));
                let visible_row = header_row.checked_sub(scroll)?;
                (visible_row < usize::from(content.height)).then_some((
                    Rect::new(
                        content.right().saturating_sub(button_width),
                        content.y.saturating_add(visible_row as u16),
                        button_width,
                        1,
                    ),
                    HitId::NotificationCopy(notice.id),
                ))
            })
            .collect()
    }

    fn render_panel(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &NotificationPanelCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        let spec = self.modal_spec_with_hints(ctx.hints);
        let Some(areas) = render_pane(frame, area, &spec, ctx.theme) else {
            return;
        };
        let items = self.modal_items(ctx.session_id);
        self.visible_count.set(items.len());
        if items.is_empty() {
            self.max_scroll.set(0);
            self.selected.set(0);
            self.item_offsets.borrow_mut().clear();
            frame.render_widget(
                Paragraph::new("No notices in this scope.")
                    .style(Style::default().fg(ctx.theme.muted)),
                areas.content,
            );
            return;
        }

        let selected = self.selected.get().min(items.len().saturating_sub(1));
        self.selected.set(selected);
        let mut lines = Vec::new();
        let mut offsets = Vec::with_capacity(items.len());
        for (index, notice) in items.into_iter().enumerate() {
            let start = lines.len();
            lines.extend(notification_lines(
                notice,
                ctx.now,
                ctx.theme,
                areas.content.width,
                index == selected,
                interaction.hovered == Some(HitId::NotificationCopy(notice.id)),
                self.is_copied(notice.id, ctx.now),
            ));
            offsets.push((notice.id, start, lines.len()));
        }
        let max_scroll = lines
            .len()
            .saturating_sub(usize::from(areas.content.height));
        self.max_scroll.set(max_scroll);
        self.viewport_height.set(usize::from(areas.content.height));
        *self.item_offsets.borrow_mut() = offsets;
        frame.render_widget(
            Paragraph::new(lines).scroll((self.scroll.min(max_scroll) as u16, 0)),
            areas.content,
        );
    }
}

impl Component<HitId, NotificationPanelCtx<'_>> for NotificationCenter {
    fn render(&self, frame: &mut Frame<'_>, area: Rect, ctx: &NotificationPanelCtx<'_>) {
        self.render_panel(frame, area, ctx, InteractionState::default());
    }

    fn render_with_state(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        ctx: &NotificationPanelCtx<'_>,
        interaction: InteractionState<HitId>,
    ) {
        self.render_panel(frame, area, ctx, interaction);
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

fn notification_lines(
    notification: &Notification,
    now: Instant,
    theme: &Theme,
    width: u16,
    selected: bool,
    copy_hovered: bool,
    copied: bool,
) -> Vec<Line<'static>> {
    let color = match notification.level {
        NotificationLevel::Info => theme.info,
        NotificationLevel::Warning => theme.warning,
        NotificationLevel::Error => theme.error,
    };
    let glyph = level_glyph(notification.level);
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
    let header_bg = selected.then_some(theme.bg_selected);
    let base = |foreground| {
        let style = Style::default().fg(foreground);
        header_bg.map_or(style, |background| style.bg(background))
    };
    let button_width = usize::from(COPY_LABEL_WIDTH.min(width));
    let left_width = usize::from(width).saturating_sub(button_width);
    let metadata = format!("{scope} · {policy} · {status}");
    let metadata = truncate_paint_cols(&metadata, left_width.saturating_sub(2));
    let used = 2usize.saturating_add(paint_cols(&metadata));
    let padding = left_width.saturating_sub(used);
    let label = if copied { COPIED_LABEL } else { COPY_LABEL };
    let button = truncate_paint_cols(&format!("{label:>button_width$}"), button_width);
    let button_style = if copied {
        base(theme.success)
    } else if copy_hovered {
        Style::default().fg(theme.text).bg(theme.bg_hover)
    } else {
        base(theme.muted)
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{glyph} "), base(color)),
        Span::styled(metadata, base(theme.dim)),
        Span::styled(" ".repeat(padding), base(theme.dim)),
        Span::styled(button, button_style),
    ])];
    let message_width = usize::from(width).saturating_sub(2).max(1);
    lines.extend(
        soft_wrap(&notification.message, message_width)
            .into_iter()
            .map(|row| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(row, Style::default().fg(theme.text)),
                ])
            }),
    );
    lines
}

const COPY_LABEL: &str = "[Copy]";
const COPIED_LABEL: &str = "[Copied]";
const COPY_LABEL_WIDTH: u16 = 8;

fn notification_height(notification: &Notification, width: u16) -> usize {
    1usize.saturating_add(
        soft_wrap(
            &notification.message,
            usize::from(width).saturating_sub(2).max(1),
        )
        .len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::line_layout::paint_cols;

    fn notice(level: NotificationLevel, message: &str) -> Notification {
        Notification {
            id: 1,
            level,
            scope: NoticeScope::Global,
            policy: NoticePolicy::Dismissible,
            status: NoticeStatus::Active,
            message: message.to_string(),
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn long_messages_wrap_with_an_indented_continuation() {
        let lines = notification_lines(
            &notice(NotificationLevel::Info, "abcdefghijklmnop"),
            Instant::now(),
            &Theme::dark(),
            10,
            false,
            false,
            false,
        );

        assert!(line_text(&lines[0]).starts_with("ⓘ "));
        assert!(line_text(&lines[0]).ends_with("[Copy]"));
        assert_eq!(line_text(&lines[1]), "  abcdefgh");
        assert_eq!(line_text(&lines[2]), "  ijklmnop");
        assert!(
            lines[1..]
                .iter()
                .all(|line| paint_cols(&line_text(line)) <= 10)
        );
    }

    #[test]
    fn severity_uses_distinct_glyphs_without_level_words() {
        let theme = Theme::dark();
        let now = Instant::now();
        let rendered = [
            (NotificationLevel::Info, "ⓘ"),
            (NotificationLevel::Warning, "▲"),
            (NotificationLevel::Error, "✗"),
        ];

        for (level, glyph) in rendered {
            let text = line_text(
                &notification_lines(
                    &notice(level, "message"),
                    now,
                    &theme,
                    40,
                    false,
                    false,
                    false,
                )[0],
            );
            assert!(text.starts_with(glyph), "{text}");
            assert!(!text.contains("info") && !text.contains("warning") && !text.contains("error"));
        }
    }

    #[test]
    fn copied_feedback_replaces_the_button_label() {
        let lines = notification_lines(
            &notice(NotificationLevel::Info, "message"),
            Instant::now(),
            &Theme::dark(),
            40,
            false,
            false,
            true,
        );

        assert!(line_text(&lines[0]).ends_with("[Copied]"));
    }
}
