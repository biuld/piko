use super::*;
use crate::ui::line_layout::paint_cols;
use piko_tui_layout::{Padding, intersection};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

/// Join title affixes for the right title segment (double-space separation).
pub fn format_title_affixes(affixes: &[PaneTitleAffix]) -> String {
    affixes
        .iter()
        .map(PaneTitleAffix::display)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

/// One prepared Pane geometry snapshot.  Every chrome zone and affix hit is
/// derived once and can be shared by paint and pointer composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanePlan {
    pub outer: Rect,
    /// Block interior before the Pane's content padding is applied.
    pub frame: Rect,
    pub title: Option<Rect>,
    pub search: Option<Rect>,
    pub search_rule: Option<Rect>,
    pub content: Rect,
    pub tip: Option<Rect>,
    pub footer: Option<Rect>,
    /// The area in which a child may paint or expose content hits.
    pub clip: Rect,
    pub affix_hits: Vec<(Rect, PaneAffixHit)>,
}

/// Compatibility view returned by [`render_pane`].
#[derive(Clone, Copy, Debug)]
pub struct PaneAreas {
    /// Main body (list, table, form, …).
    pub content: Rect,
    /// Present only for [`PaneFooter::Reserved`].
    pub footer: Option<Rect>,
    /// Full block interior (search + content + tip + footer).
    #[allow(dead_code)]
    pub inner: Rect,
}

/// Prepare Pane chrome geometry without painting.
pub fn prepare_pane(area: Rect, spec: &PaneSpec<'_>) -> Option<PanePlan> {
    let frame = Block::default().borders(spec.borders).inner(area);
    if frame.width == 0 || frame.height == 0 {
        return None;
    }
    let pad = spec.padding;
    let zone_x = frame.x.saturating_add(pad.horizontal);
    let zone_w = frame.width.saturating_sub(pad.horizontal.saturating_mul(2));
    if zone_w == 0 {
        return None;
    }

    let title = (area.height > 0 && (!spec.title.is_empty() || !spec.title_affixes.is_empty()))
        .then_some(Rect::new(area.x, area.y, area.width, 1));
    let affix_hits = title_affix_hits(area, spec);
    let show_search = !matches!(spec.search, PaneSearch::Hidden);
    let show_rule = show_search && spec.search_rule;
    let show_tip = spec.tip.is_some_and(|tip| !tip.is_empty());
    let footer_h = footer_height(spec.footer);

    // Content starts below the top padding.
    let content_top = frame.y.saturating_add(pad.vertical);
    // Footer anchors to the frame bottom (flush to the border). The vertical
    // padding doubles as the separator between content/tip and the footer;
    // without a footer it leaves a breathing row.
    let footer_top = frame
        .y
        .saturating_add(frame.height.saturating_sub(footer_h));
    let gap_top = footer_top.saturating_sub(pad.vertical);
    let tip_top = gap_top.saturating_sub(u16::from(show_tip));

    let mut y = content_top;
    let search = show_search.then(|| {
        let rect = Rect::new(zone_x, y, zone_w, 1);
        y = y.saturating_add(1);
        rect
    });
    let search_rule = show_rule.then(|| {
        let rect = Rect::new(zone_x, y, zone_w, 1);
        y = y.saturating_add(1);
        rect
    });
    let content_height = tip_top.saturating_sub(y);
    let content = Rect::new(zone_x, y, zone_w, content_height);
    let tip = show_tip.then_some(Rect::new(zone_x, tip_top, zone_w, 1));
    let footer = (footer_h > 0).then_some(Rect::new(zone_x, footer_top, zone_w, footer_h));
    Some(PanePlan {
        outer: area,
        frame,
        title,
        search,
        search_rule,
        content,
        tip,
        footer,
        clip: content,
        affix_hits,
    })
}

/// Paint a previously prepared Pane plan.
pub fn paint_pane(frame: &mut Frame<'_>, plan: &PanePlan, spec: &PaneSpec<'_>, theme: &Theme) {
    frame.render_widget(Clear, plan.outer);
    if let Some(color) = spec.fill {
        frame.render_widget(
            Block::default().style(Style::default().bg(color)),
            plan.outer,
        );
    }

    let block = pane_block(spec, theme);
    frame.render_widget(block, plan.outer);

    if let Some(area) = plan.search {
        paint_search(frame, area, &spec.search, theme);
    }
    if let Some(area) = plan.search_rule {
        paint_rule(frame, area, theme);
    }
    if let Some(area) = plan.tip
        && let Some(tip) = spec.tip
    {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                tip.to_string(),
                Style::default().fg(theme.muted),
            ))),
            area,
        );
    }
    if let Some(area) = plan.footer
        && let PaneFooter::Hints(hints) = spec.footer
    {
        paint_hints(frame, area, hints, theme);
    }
}

/// Clear area, draw chrome, and return the compatibility body view.
pub fn render_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: &PaneSpec<'_>,
    theme: &Theme,
) -> Option<PaneAreas> {
    let plan = prepare_pane(area, spec)?;
    paint_pane(frame, &plan, spec, theme);
    Some(PaneAreas {
        content: plan.content,
        footer: matches!(spec.footer, PaneFooter::Reserved { .. })
            .then_some(plan.footer)
            .flatten(),
        inner: inset_xy(plan.frame, spec.padding),
    })
}

fn pane_block(spec: &PaneSpec<'_>, theme: &Theme) -> Block<'static> {
    let mut block = Block::default()
        .borders(spec.borders)
        .border_style(frame_border_style(spec.focused, theme));
    if !spec.title.is_empty() {
        block = block.title(
            Line::from(Span::styled(
                format!(" {} ", spec.title),
                Style::default().fg(theme.text),
            ))
            .alignment(Alignment::Left),
        );
    }
    if !spec.title_affixes.is_empty() {
        let right = format_title_affixes(&spec.title_affixes);
        if !right.is_empty() {
            block = block.title(
                Line::from(Span::styled(
                    format!(" {right} "),
                    Style::default().fg(theme.dim),
                ))
                .alignment(Alignment::Right),
            );
        }
    }

    block
}

fn title_affix_hits(area: Rect, spec: &PaneSpec<'_>) -> Vec<(Rect, PaneAffixHit)> {
    if area.width == 0 || area.height == 0 || spec.title_affixes.is_empty() {
        return Vec::new();
    }
    let title_area = Rect::new(area.x, area.y, area.width, 1);
    let displays: Vec<String> = spec
        .title_affixes
        .iter()
        .map(PaneTitleAffix::display)
        .collect();
    let cluster = displays.join("  ");
    let cluster_width =
        crate::ui::line_layout::paint_cols(&cluster).min(usize::from(u16::MAX)) as u16;
    let mut x = area
        .x
        .saturating_add(area.width.saturating_sub(cluster_width.saturating_add(2)));
    let mut hits = Vec::new();

    for (affix, display) in spec.title_affixes.iter().zip(displays) {
        match affix {
            PaneTitleAffix::Close => {
                push_affix_hit(
                    &mut hits,
                    title_area,
                    x,
                    paint_cols(&display),
                    PaneAffixHit::Close,
                );
            }
            PaneTitleAffix::ModeStrip(strip) => {
                let mut option_x = x;
                for (index, option) in strip.options.iter().enumerate() {
                    let width = paint_cols(option)
                        .saturating_add((index == strip.clamped_active()) as usize * 2);
                    push_affix_hit(
                        &mut hits,
                        title_area,
                        option_x,
                        width,
                        PaneAffixHit::ModeOption(index),
                    );
                    option_x = option_x
                        .saturating_add(width.min(usize::from(u16::MAX)) as u16)
                        .saturating_add(3);
                }
            }
            PaneTitleAffix::Label(_) | PaneTitleAffix::Selection { .. } => {}
        }
        x = x
            .saturating_add(paint_cols(&display).min(usize::from(u16::MAX)) as u16)
            .saturating_add(2);
    }
    hits
}

fn push_affix_hit(
    hits: &mut Vec<(Rect, PaneAffixHit)>,
    title_area: Rect,
    x: u16,
    width: usize,
    hit: PaneAffixHit,
) {
    let rect = Rect::new(x, title_area.y, width.min(usize::from(u16::MAX)) as u16, 1);
    if let Some(rect) = intersection(title_area, rect)
        && rect.width > 0
    {
        hits.push((rect, hit));
    }
}

pub(super) fn inset_xy(area: Rect, pad: PanePadding) -> Rect {
    let hx = pad.horizontal.min(area.width.saturating_sub(1) / 2);
    let vy = pad.vertical.min(area.height.saturating_sub(1) / 2);
    Padding::new(vy, hx, vy, hx).apply(area)
}

pub(super) fn footer_height(footer: PaneFooter<'_>) -> u16 {
    match footer {
        PaneFooter::None => 0,
        PaneFooter::Hints(hints) => u16::from(!hints.is_empty()),
        PaneFooter::Reserved { height } => height.max(1),
    }
}

fn paint_search(frame: &mut Frame<'_>, area: Rect, search: &PaneSearch<'_>, theme: &Theme) {
    match search {
        PaneSearch::Hidden => {}
        PaneSearch::Custom(line) => {
            frame.render_widget(Paragraph::new(line.clone()), area);
        }
        PaneSearch::Shown {
            filter,
            placeholder,
        } => {
            // Product convention: `/` glyph + dim placeholder when empty,
            // `/ <filter>` with accent filter while typing.
            let placeholder = placeholder.unwrap_or(SEARCH_PLACEHOLDER);
            let line = if filter.is_empty() {
                Line::from(vec![Span::styled(
                    format!("{SEARCH_GLYPH} {placeholder}"),
                    Style::default().fg(theme.dim),
                )])
            } else {
                Line::from(vec![
                    Span::styled(format!("{SEARCH_GLYPH} "), Style::default().fg(theme.muted)),
                    Span::styled(filter.to_string(), Style::default().fg(theme.accent)),
                ])
            };
            frame.render_widget(Paragraph::new(line), area);
        }
    }
}

fn paint_rule(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
    let width = area.width.max(1) as usize;
    let rule = "─".repeat(width);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(theme.dim),
        ))),
        area,
    );
}

fn paint_hints(frame: &mut Frame<'_>, area: Rect, hints: InteractionHints<'_>, theme: &Theme) {
    let Some(hint) = hints.single_line() else {
        return;
    };
    crate::ui::components::dock_line::render(
        frame,
        area,
        crate::ui::components::dock_line::hint_line(hint, theme),
        None,
    );
}

/// Section rule line: `Appearance ────────` (label + fill with dim box-drawing).
pub fn section_rule_line(label: &str, width: usize, theme: &Theme) -> Line<'static> {
    let label = label.trim();
    let label_chars = label.chars().count();
    // Screenshot: label then contiguous fill with a single space gap.
    let fill = width.saturating_sub(label_chars).saturating_sub(1);
    let mut spans = vec![Span::styled(
        label.to_string(),
        Style::default().fg(theme.muted),
    )];
    if fill > 0 {
        spans.push(Span::styled(
            format!(" {}", "─".repeat(fill)),
            Style::default().fg(theme.dim),
        ));
    }
    Line::from(spans)
}
