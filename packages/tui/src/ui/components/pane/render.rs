use super::*;

/// Join title affixes for the right title segment (double-space separation).
pub fn format_title_affixes(affixes: &[PaneTitleAffix]) -> String {
    affixes
        .iter()
        .map(PaneTitleAffix::display)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

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

/// Clear area, draw border/title/search/tip/footer chrome, return body rects.
///
/// Returns `None` when the area is too small to show content.
pub fn render_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    spec: &PaneSpec<'_>,
    theme: &Theme,
) -> Option<PaneAreas> {
    frame.render_widget(Clear, area);
    if let Some(color) = spec.fill {
        frame.render_widget(Block::default().style(Style::default().bg(color)), area);
    }

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

    let bordered = block.inner(area);
    frame.render_widget(block, area);

    let inner = inset_xy(bordered, spec.padding);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    let show_search = !matches!(spec.search, PaneSearch::Hidden);
    let show_rule = show_search && spec.search_rule;
    let show_tip = spec.tip.is_some_and(|t| !t.is_empty());
    let footer_h = footer_height(spec.footer);

    let search_h: u16 = u16::from(show_search);
    let rule_h: u16 = u16::from(show_rule);
    let tip_h: u16 = u16::from(show_tip);
    let chrome = search_h
        .saturating_add(rule_h)
        .saturating_add(tip_h)
        .saturating_add(footer_h);

    if inner.height <= chrome {
        return paint_fallback(frame, inner, spec, theme, footer_h);
    }

    let mut constraints: Vec<Constraint> = Vec::with_capacity(5);
    if show_search {
        constraints.push(Constraint::Length(1));
    }
    if show_rule {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(1));
    if show_tip {
        constraints.push(Constraint::Length(1));
    }
    if footer_h > 0 {
        constraints.push(Constraint::Length(footer_h));
    }

    let chunks = Layout::vertical(constraints).split(inner);
    let mut idx = 0usize;

    if show_search {
        paint_search(frame, chunks[idx], &spec.search, theme);
        idx += 1;
    }
    if show_rule {
        paint_rule(frame, chunks[idx], theme);
        idx += 1;
    }

    let content = chunks[idx];
    idx += 1;

    if show_tip {
        if let Some(tip) = spec.tip {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    tip.to_string(),
                    Style::default().fg(theme.muted),
                ))),
                chunks[idx],
            );
        }
        idx += 1;
    }

    let footer = if footer_h > 0 {
        let footer_area = chunks[idx];
        match spec.footer {
            PaneFooter::Hints(hints) => {
                paint_hints(frame, footer_area, hints, theme);
                None
            }
            PaneFooter::Reserved { .. } => Some(footer_area),
            PaneFooter::None => None,
        }
    } else {
        None
    };

    Some(PaneAreas {
        content,
        footer,
        inner,
    })
}

pub(super) fn inset_xy(area: Rect, pad: PanePadding) -> Rect {
    let hx = pad.horizontal.min(area.width.saturating_sub(1) / 2);
    let vy = pad.vertical.min(area.height.saturating_sub(1) / 2);
    Rect {
        x: area.x.saturating_add(hx),
        y: area.y.saturating_add(vy),
        width: area.width.saturating_sub(hx.saturating_mul(2)),
        height: area.height.saturating_sub(vy.saturating_mul(2)),
    }
}

pub(super) fn footer_height(footer: PaneFooter<'_>) -> u16 {
    match footer {
        PaneFooter::None => 0,
        PaneFooter::Hints(hints) => u16::from(!hints.is_empty()),
        PaneFooter::Reserved { height } => height.max(1),
    }
}

fn paint_fallback(
    frame: &mut Frame<'_>,
    inner: Rect,
    spec: &PaneSpec<'_>,
    theme: &Theme,
    footer_h: u16,
) -> Option<PaneAreas> {
    if footer_h > 0 && inner.height > footer_h {
        let chunks =
            Layout::vertical([Constraint::Min(1), Constraint::Length(footer_h)]).split(inner);
        let footer = match spec.footer {
            PaneFooter::Hints(hints) => {
                paint_hints(frame, chunks[1], hints, theme);
                None
            }
            PaneFooter::Reserved { .. } => Some(chunks[1]),
            PaneFooter::None => None,
        };
        Some(PaneAreas {
            content: chunks[0],
            footer,
            inner,
        })
    } else if inner.height >= 1 {
        Some(PaneAreas {
            content: inner,
            footer: None,
            inner,
        })
    } else {
        None
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
