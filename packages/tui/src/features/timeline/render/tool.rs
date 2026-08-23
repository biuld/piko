use super::*;

pub(super) fn tool_lines(
    tool: &ToolEntry,
    hovered: bool,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let presented = present_tool(
        &tool.name,
        &tool.args,
        tool.result.as_deref(),
        tool.result_details.as_deref(),
    );
    // Card tone: command badge (exit code) wins over protocol ToolStatus for shell tools.
    let bg = card_bg(tool.status, presented.title_badge.as_ref(), theme);
    let title_style = Style::default()
        .fg(if hovered {
            theme.accent
        } else {
            theme.tool_title
        })
        .add_modifier(Modifier::BOLD)
        .bg(bg);
    let output_style = Style::default().fg(theme.tool_output).bg(bg);
    let muted_style = Style::default().fg(theme.dim).bg(bg);
    let title_meta = presented.title_meta.as_deref();
    let tokens = estimate_tool_result_tokens(tool);

    // Todo strip is live truth (F-27); timeline todo_* cards are audit only —
    // do not force-expand checklist bodies when the strip path is enabled.
    let show_body = tool.expanded;

    // Always bookend with pad rows so collapsed cards stay 3 lines tall.
    let mut lines = vec![
        filled_line("", output_style, width),
        tool_title_line(ToolTitle {
            expanded: show_body,
            name: &tool.name,
            meta: title_meta,
            status: tool.status,
            badge: presented.title_badge.as_ref(),
            tokens,
            title_style,
            meta_style: muted_style,
            theme,
            bg,
            width,
        }),
    ];

    if show_body {
        if !matches!(presented.body, ToolBody::Empty) {
            lines.push(filled_line("", output_style, width));
            lines.extend(render_tool_body(&presented.body, theme, bg, width));
        }
        lines.push(filled_line("", output_style, width));
    } else {
        // Collapsed middle/content row keeps card at 3 lines when no body.
        // Prefer title_meta on the title; only use a second content row when
        // there is a distinct collapsed preview and no title meta.
        if title_meta.is_none() && !presented.collapsed_preview.is_empty() {
            // Replace the forced 3-line pattern: pad · title · preview
            // (preview is the third row; bottom pad omitted to stay at 3).
            // Long previews soft-wrap instead of getting clipped.
            lines.extend(prefixed_wrap(
                vec![Span::styled(" ", output_style)],
                &presented.collapsed_preview,
                output_style,
                output_style,
                width,
            ));
        } else {
            lines.push(filled_line("", output_style, width));
        }
    }
    lines
}

struct ToolTitle<'a> {
    expanded: bool,
    name: &'a str,
    meta: Option<&'a str>,
    status: ToolStatus,
    badge: Option<&'a TitleBadge>,
    tokens: Option<u64>,
    title_style: Style,
    meta_style: Style,
    theme: &'a Theme,
    bg: Color,
    width: u16,
}

fn card_bg(status: ToolStatus, badge: Option<&TitleBadge>, theme: &Theme) -> Color {
    if let Some(badge) = badge {
        return match badge.tone {
            BadgeTone::Success => theme.tool_success_bg,
            BadgeTone::Error => theme.tool_error_bg,
            BadgeTone::Warning => theme.tool_error_bg,
            BadgeTone::Running => theme.tool_pending_bg,
            BadgeTone::Neutral => match status {
                ToolStatus::Running => theme.tool_pending_bg,
                ToolStatus::Failed | ToolStatus::Cancelled => theme.tool_error_bg,
                ToolStatus::Completed => theme.tool_success_bg,
            },
        };
    }
    match status {
        ToolStatus::Running => theme.tool_pending_bg,
        ToolStatus::Completed => theme.tool_success_bg,
        ToolStatus::Failed | ToolStatus::Cancelled => theme.tool_error_bg,
    }
}

fn badge_fg(tone: BadgeTone, theme: &Theme) -> Color {
    match tone {
        BadgeTone::Success => theme.success,
        BadgeTone::Error => theme.error,
        BadgeTone::Warning => theme.warning,
        BadgeTone::Running => theme.running,
        BadgeTone::Neutral => theme.dim,
    }
}

/// Minimum blank columns between left content and the right status cluster.
const TITLE_ZONE_SPACER: usize = 3;
/// Trailing margin after the right cluster.
const TITLE_RIGHT_MARGIN: usize = 1;
/// Title row zones (terminal columns):
/// ```text
/// | left (truncated)              | spacer≥3 | right (never truncated) | margin |
/// | ▸ exec  $ cargo fmt --all...|           | exit 127 · 60ms · ~1.2k |        |
/// ```
///
/// Right is reserved first (full chips). Widths use [`paint_cols`]
/// (`unicode-width`). Final fill also uses paint width so the row bg is solid.
fn tool_title_line(spec: ToolTitle<'_>) -> Line<'static> {
    let target = usize::from(spec.width);
    if target == 0 {
        return Line::from("");
    }

    let mark = if spec.expanded {
        DISCLOSURE_EXPANDED
    } else {
        DISCLOSURE_COLLAPSED
    };

    // ── Right cluster (immutable once built) ────────────────────────────────
    let (badge_text, badge_style) = if let Some(badge) = spec.badge {
        (
            badge.text.clone(),
            Style::default()
                .fg(badge_fg(badge.tone, spec.theme))
                .bg(spec.bg),
        )
    } else {
        let (label, fg) = match spec.status {
            ToolStatus::Running => (RUNNING_GLYPH, spec.theme.running),
            ToolStatus::Completed => (SUCCESS_GLYPH, spec.theme.success),
            ToolStatus::Failed => (FAIL_GLYPH, spec.theme.error),
            ToolStatus::Cancelled => (CANCELLED_GLYPH, spec.theme.warning),
        };
        (label.to_string(), Style::default().fg(fg).bg(spec.bg))
    };
    let dim = Style::default().fg(spec.theme.dim).bg(spec.bg);

    // Chips: status/badge → duration → tokens (tokens last, whole chip reserved).
    let mut right_chips: Vec<(String, Style)> = vec![(badge_text, badge_style)];
    if let Some(dur) = spec.badge.and_then(|b| b.duration.as_ref()) {
        right_chips.push((dur.clone(), dim));
    }
    if let Some(n) = spec.tokens {
        right_chips.push((format!("~{}", piko_client_core::format_tokens(n)), dim));
    }

    let right_content_w: usize = right_chips
        .iter()
        .enumerate()
        .map(|(i, (text, _))| {
            let sep = if i > 0 { paint_cols(CHIP_SEP) } else { 0 };
            sep + paint_cols(text)
        })
        .sum();
    let right_block_w = right_content_w.saturating_add(TITLE_RIGHT_MARGIN);

    // ── Left: remainder after right + minimum spacer ────────────────────────
    let left_max = target
        .saturating_sub(right_block_w)
        .saturating_sub(TITLE_ZONE_SPACER);

    let core = format!(" {mark} {}", spec.name);
    // Meta stays content text (paths, cmds, prompts). Strip only known box-art
    // / flow noise that used to live in spawn titles — not status glyphs.
    let safe_meta = spec.meta.map(sanitize_title_meta);
    let (left_text, meta_span) = fit_title_left(core, safe_meta.as_deref(), left_max);
    let left_w = paint_cols(&left_text) + meta_span.as_ref().map(|m| paint_cols(m)).unwrap_or(0);

    let spacer = target.saturating_sub(left_w).saturating_sub(right_block_w);

    let mut spans: Vec<Span<'static>> = Vec::new();
    if !left_text.is_empty() {
        spans.push(Span::styled(left_text, spec.title_style));
    }
    if let Some(meta) = meta_span {
        spans.push(Span::styled(meta, spec.meta_style));
    }
    if spacer > 0 {
        spans.push(Span::styled(" ".repeat(spacer), spec.title_style));
    }

    if left_w + right_block_w <= target {
        for (i, (text, style)) in right_chips.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(CHIP_SEP.to_string(), dim));
            }
            spans.push(Span::styled(text, style));
        }
        if TITLE_RIGHT_MARGIN > 0 {
            spans.push(Span::styled(" ".repeat(TITLE_RIGHT_MARGIN), dim));
        }
    } else {
        // Ultra-narrow: whole trailing chips only (prefer tokens over badge).
        let remain = target.saturating_sub(left_w);
        let fitted = fit_right_chips_from_end(&right_chips, remain);
        for (i, (text, style)) in fitted.into_iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(CHIP_SEP.to_string(), dim));
            }
            spans.push(Span::styled(text, style));
        }
    }

    let used: usize = spans.iter().map(|s| paint_cols(s.content.as_ref())).sum();
    if used < target {
        spans.push(Span::styled(
            " ".repeat(target - used),
            Style::default().bg(spec.bg),
        ));
    }
    Line::from(spans)
}

/// Prefer trailing chips (tokens) when the right zone cannot fit everything.
fn fit_right_chips_from_end(chips: &[(String, Style)], max_cols: usize) -> Vec<(String, Style)> {
    if max_cols == 0 || chips.is_empty() {
        return Vec::new();
    }
    for start in 0..chips.len() {
        let slice = &chips[start..];
        let mut w = 0usize;
        for (i, (text, _)) in slice.iter().enumerate() {
            if i > 0 {
                w = w.saturating_add(paint_cols(CHIP_SEP));
            }
            w = w.saturating_add(paint_cols(text));
        }
        if w <= max_cols {
            return slice.to_vec();
        }
    }
    let (text, style) = chips.last().expect("non-empty");
    let clipped = truncate_cols(text, max_cols);
    if clipped.is_empty() {
        Vec::new()
    } else {
        vec![(clipped, *style)]
    }
}

/// Strip legacy flow / box-drawing noise from title meta. Does **not** touch
/// normal punctuation; Chinese and paths pass through.
fn sanitize_title_meta(meta: &str) -> String {
    let mut out = String::with_capacity(meta.len());
    for ch in meta.chars() {
        match ch {
            // Ambiguous separators that used to appear in spawn chrome.
            '·' | '•' | '∙' | '・' => out.push('.'),
            '…' => out.push_str("..."),
            // Flow / tree art — never belongs on the title meta line.
            '▸' | '▹' | '►' | '▶' | '▷' | '→' | '⇒' => out.push('>'),
            '◂' | '◃' | '◄' | '◀' | '◁' | '←' | '⇐' => out.push('<'),
            '◆' | '◇' | '●' | '○' | '■' | '□' | '★' | '☆' | '⬤' => out.push('*'),
            '─' | '━' | '┄' | '┅' | '┈' | '┉' | '╌' | '╍' | '═' => out.push('-'),
            '│' | '┃' | '┊' | '┋' | '╎' | '╏' | '║' => out.push('|'),
            '└' | '┌' | '┐' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' | '╭' | '╮' | '╯' | '╰' => {
                out.push('+')
            }
            other => out.push(other),
        }
    }
    out
}

/// Fit `core` (+ optional meta) into `max_cols`, truncating meta first, then core.
fn fit_title_left(core: String, meta: Option<&str>, max_cols: usize) -> (String, Option<String>) {
    if max_cols == 0 {
        return (String::new(), None);
    }
    let core_w = paint_cols(&core);
    if core_w > max_cols {
        return (truncate_cols(&core, max_cols), None);
    }
    let Some(meta) = meta.filter(|m| !m.is_empty()) else {
        return (core, None);
    };
    let meta_budget = max_cols - core_w;
    // Need "  " + something visible.
    if meta_budget < 3 {
        return (core, None);
    }
    let meta_part = truncate_cols(&format!("  {meta}"), meta_budget);
    (core, Some(meta_part))
}

/// Per-tool token count for the title right zone.
///
/// Prefer real provider usage embedded in the tool result (e.g. spawn
/// `usage` / `outcome.usage`). Fall back to a chars/4 size heuristic only when
/// no usage payload is present (read/exec and similar).
fn estimate_tool_result_tokens(tool: &ToolEntry) -> Option<u64> {
    for text in [tool.result.as_deref(), tool.result_details.as_deref()]
        .into_iter()
        .flatten()
    {
        if let Some(n) = usage_tokens_from_json_text(text).filter(|n| *n > 0) {
            return Some(n);
        }
    }
    let mut chars = 0usize;
    if let Some(result) = &tool.result {
        chars = chars.saturating_add(result.chars().count());
    }
    if let Some(details) = &tool.result_details {
        chars = chars.saturating_add(details.chars().count());
    }
    if chars == 0 {
        return None;
    }
    Some(((chars as f64) / 4.0).ceil() as u64)
}

/// Pull a token total from JSON that carries a `Usage`-shaped object.
fn usage_tokens_from_json_text(text: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    usage_tokens_from_value(&value)
}

fn usage_tokens_from_value(value: &serde_json::Value) -> Option<u64> {
    // Common placements: top-level `usage`, or ExecutionOutcome `outcome.usage`.
    if let Some(n) = usage_object_tokens(value.get("usage")) {
        return Some(n);
    }
    if let Some(outcome) = value.get("outcome")
        && let Some(n) = usage_object_tokens(outcome.get("usage"))
    {
        return Some(n);
    }
    // collect_agent_reports / list payloads: first nested report with usage.
    for key in ["reports", "items", "consumed", "agents"] {
        if let Some(items) = value.get(key).and_then(|v| v.as_array()) {
            let mut sum = 0u64;
            let mut any = false;
            for item in items {
                let report = item.get("report").unwrap_or(item);
                if let Some(n) = usage_tokens_from_value(report) {
                    sum = sum.saturating_add(n);
                    any = true;
                }
            }
            if any {
                return Some(sum);
            }
        }
    }
    None
}

fn usage_object_tokens(value: Option<&serde_json::Value>) -> Option<u64> {
    let value = value?;
    if !value.is_object() {
        return None;
    }
    // Prefer protocol Usage (camelCase). Tolerate snake_case aliases if present.
    let total = u64_field(value, "totalTokens")
        .or_else(|| u64_field(value, "total_tokens"))
        .unwrap_or(0);
    if total > 0 {
        return Some(total);
    }
    let input = u64_field(value, "input").unwrap_or(0);
    let output = u64_field(value, "output").unwrap_or(0);
    let cache_read = u64_field(value, "cacheRead")
        .or_else(|| u64_field(value, "cache_read"))
        .unwrap_or(0);
    let cache_write = u64_field(value, "cacheWrite")
        .or_else(|| u64_field(value, "cache_write"))
        .unwrap_or(0);
    let sum = input
        .saturating_add(output)
        .saturating_add(cache_read)
        .saturating_add(cache_write);
    (sum > 0).then_some(sum)
}

fn u64_field(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| {
        v.as_u64()
            .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
            .or_else(|| v.as_f64().map(|n| n.max(0.0) as u64))
    })
}
