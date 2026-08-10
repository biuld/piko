//! Empty-stream welcome banner shown when the timeline has no paintables.
//!
//! Renders a bordered, centered card with a multi-style ASCII logo, meta row,
//! and tip list. Glyph art and logo-style cycling live in [`glyphs`].

mod glyphs;

use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;

use self::glyphs::{
    LogoStyle, WAVE_TICKS, logo_band_rows, logo_region_height, logo_region_width, pick_logo_style,
    sparkle_glyph,
};

/// Context for the empty-timeline welcome surface.
pub struct WelcomeView<'a> {
    pub version: &'a str,
    pub cwd: &'a Path,
    /// Advances on the app tick; drives logo style cycle + highlight wave.
    pub spinner_frame: usize,
}

/// Preferred inner width of the welcome card (excluding borders).
const CARD_INNER_W: u16 = 42;

/// Fixed body rows below the logo band (gap, meta×2, gap, hairline, gap, tips×3).
const FIXED_TAIL_LINES: u16 = 1 + 2 + 1 + 1 + 1 + 3;

/// Paint the empty-stream welcome card into `area`.
pub fn render(frame: &mut Frame<'_>, area: Rect, theme: &Theme, view: WelcomeView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let style = pick_logo_style(view.spinner_frame, area.width.saturating_sub(4));
    // Card size is independent of the active logo style (fixed logo band).
    let body_h = (logo_region_height() as u16).saturating_add(FIXED_TAIL_LINES);
    // border (2) + vertical pad (2)
    let card_h = body_h.saturating_add(4).min(area.height);
    let card_w = CARD_INNER_W
        .saturating_add(4) // borders + horizontal pad
        .min(area.width)
        .max(16);

    let card = center_rect(area, card_w, card_h);
    frame.render_widget(Clear, card);

    let border_style = border_style_for_frame(view.spinner_frame, theme);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(card);
    frame.render_widget(block, card);

    // 1-cell pad inside the border; body uses the true content width.
    let padded = inset(inner, 1, 1);
    if padded.width == 0 || padded.height == 0 {
        return;
    }
    let body = card_body_lines(theme, &view, style, padded.width);

    frame.render_widget(Paragraph::new(body).alignment(Alignment::Left), padded);
}

/// Build card body lines (no border). Logo occupies a fixed-height band.
fn card_body_lines(
    theme: &Theme,
    view: &WelcomeView<'_>,
    style: LogoStyle,
    inner_width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let content_w = inner_width.max(8);

    lines.extend(logo_region_lines(theme, view, style, content_w));
    lines.push(Line::from(""));

    // Tagline + version, centered-ish as a single meta line.
    let meta = format!("coding agent · v{}", view.version);
    lines.push(center_line(
        content_w,
        vec![Span::styled(
            meta,
            Style::default().fg(theme.text_secondary),
        )],
    ));

    let cwd = display_cwd(view.cwd, content_w.saturating_sub(2).max(8) as usize);
    lines.push(center_line(
        content_w,
        vec![Span::styled(cwd, Style::default().fg(theme.dim))],
    ));

    lines.push(Line::from(""));
    lines.push(hairline(content_w, theme));
    lines.push(Line::from(""));
    lines.extend(tip_lines(theme, content_w));

    lines
}

/// Fixed-height logo band: style changes only rewrite glyphs inside this region.
fn logo_region_lines(
    theme: &Theme,
    view: &WelcomeView<'_>,
    style: LogoStyle,
    content_w: u16,
) -> Vec<Line<'static>> {
    let region_h = logo_region_height();
    let region_w = logo_region_width().min(content_w);
    let band_left = content_w.saturating_sub(region_w) / 2;

    let logo_w = style.min_width();
    if content_w >= logo_w && content_w >= region_w.min(logo_w) {
        let band = logo_band_rows(style);
        // Wave only across non-empty logo rows (not blank pad rows).
        let glyph_rows: Vec<usize> = band
            .iter()
            .enumerate()
            .filter_map(|(i, row)| (!row.is_empty()).then_some(i))
            .collect();
        let wave_idx = if glyph_rows.is_empty() {
            0
        } else {
            (view.spinner_frame / WAVE_TICKS) % glyph_rows.len()
        };
        let wave_row = glyph_rows.get(wave_idx).copied();

        return band
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                if row.is_empty() {
                    return Line::from("");
                }
                // Center this style's glyph width inside the fixed logo band.
                let glyph_pad = region_w.saturating_sub(row.chars().count() as u16) / 2;
                let left = band_left + glyph_pad;
                let mut style_row = Style::default().fg(theme.accent);
                if Some(i) == wave_row {
                    style_row = style_row.add_modifier(Modifier::BOLD);
                } else {
                    style_row = style_row.fg(theme.accent_alt);
                }
                Line::from(Span::styled(
                    format!("{}{row}", " ".repeat(left as usize)),
                    style_row,
                ))
            })
            .collect();
    }

    // Narrow: wordmark centered in the same fixed-height band.
    let spark = sparkle_glyph(view.spinner_frame);
    let mark = format!("{spark} piko");
    let top = region_h.saturating_sub(1) / 2;
    let left = content_w.saturating_sub(mark.chars().count() as u16) / 2;
    let mut lines = vec![Line::from(""); region_h];
    if let Some(slot) = lines.get_mut(top) {
        *slot = Line::from(Span::styled(
            format!("{}{mark}", " ".repeat(left as usize)),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lines
}

fn border_style_for_frame(spinner_frame: usize, theme: &Theme) -> Style {
    // Gentle breathe between muted and default border — not accent (reserved
    // for selection / brand text).
    let phase = (spinner_frame / 8) % 2;
    Style::default().fg(if phase == 0 {
        theme.border
    } else {
        theme.border_muted
    })
}

fn tip_lines(theme: &Theme, width: u16) -> Vec<Line<'static>> {
    const TIPS: &[(&str, &str)] = &[
        ("Enter", "submit prompt"),
        ("/", "commands"),
        ("Ctrl+Q", "quit"),
    ];

    // Key column width covers longest key + gap.
    let key_col = 8u16;
    let left_pad = 2u16;
    TIPS.iter()
        .map(|(key, desc)| {
            let key_text = format!("{key:<width$}", width = key_col as usize);
            let desc_budget = width
                .saturating_sub(left_pad)
                .saturating_sub(key_col)
                .max(4) as usize;
            let desc_text = truncate_end(desc, desc_budget);
            Line::from(vec![
                Span::raw(" ".repeat(left_pad as usize)),
                Span::styled(key_text, Style::default().fg(theme.accent_alt)),
                Span::styled(desc_text, Style::default().fg(theme.dim)),
            ])
        })
        .collect()
}

fn hairline(width: u16, theme: &Theme) -> Line<'static> {
    let n = width.saturating_sub(4).max(8) as usize;
    let pad = 2usize;
    Line::from(Span::styled(
        format!("{}{}", " ".repeat(pad), "─".repeat(n)),
        Style::default().fg(theme.border_muted),
    ))
}

fn center_line(width: u16, spans: Vec<Span<'static>>) -> Line<'static> {
    let content_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (width as usize).saturating_sub(content_len) / 2;
    let mut out = Vec::with_capacity(spans.len() + 1);
    if pad > 0 {
        out.push(Span::raw(" ".repeat(pad)));
    }
    out.extend(spans);
    Line::from(out)
}

fn center_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(width) / 2;
    // Slightly above geometric center reads better in a tall stream.
    let y = area.y + area.height.saturating_sub(height) / 3;
    Rect::new(x, y, width, height)
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    let x = area.x.saturating_add(horizontal);
    let y = area.y.saturating_add(vertical);
    let width = area.width.saturating_sub(horizontal.saturating_mul(2));
    let height = area.height.saturating_sub(vertical.saturating_mul(2));
    Rect::new(x, y, width, height)
}

fn display_cwd(cwd: &Path, max_cols: usize) -> String {
    let cwd_str = cwd.to_string_lossy();
    let display = if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        if let Some(relative) = cwd_str.strip_prefix(home_str.as_ref()) {
            if relative.is_empty() {
                "~".to_string()
            } else {
                format!("~{relative}")
            }
        } else {
            cwd_str.into_owned()
        }
    } else {
        cwd_str.into_owned()
    };
    truncate_end(&display, max_cols)
}

fn truncate_end(s: &str, max_cols: usize) -> String {
    if s.chars().count() <= max_cols {
        return s.to_string();
    }
    let keep = max_cols.saturating_sub(1);
    let tail: String = s
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use std::path::PathBuf;

    fn theme() -> Theme {
        Theme::dark()
    }

    fn view<'a>(version: &'a str, cwd: &'a Path, frame: usize) -> WelcomeView<'a> {
        WelcomeView {
            version,
            cwd,
            spinner_frame: frame,
        }
    }

    fn plain(lines: &[Line<'_>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn wide_banner_includes_logo_version_and_tips() {
        let v = view("0.1.0", Path::new("/tmp/project"), 0);
        let lines = card_body_lines(&theme(), &v, LogoStyle::Slant, 42);
        let rows = plain(&lines);
        let joined = rows.join("\n");
        assert!(joined.contains("_ __"), "logo mark missing: {joined}");
        assert!(joined.contains("v0.1.0"), "version missing: {joined}");
        assert!(joined.contains("/tmp/project") || joined.contains("project"));
        assert!(joined.contains("submit prompt"));
        assert!(joined.contains("commands"));
        assert!(joined.contains("quit"));
        assert!(
            rows.iter().any(|line| {
                line.contains("Ctrl+Q") && line.contains("quit") && !line.contains("Ctrl+Qquit")
            }),
            "tip key and description must be separated: {joined}"
        );
    }

    #[test]
    fn cwd_is_truncated_for_tiny_width() {
        let long = PathBuf::from("/very/long/path/that/should/be/truncated/by/display_cwd");
        let shown = display_cwd(&long, 10);
        assert!(shown.chars().count() <= 10);
        assert!(shown.starts_with('…') || shown.chars().count() < 10);
    }

    #[test]
    fn body_height_is_stable_across_logo_styles() {
        let v = view("0.1.0", Path::new("/tmp/project"), 0);
        let heights: Vec<usize> = [
            LogoStyle::Slant,
            LogoStyle::Box,
            LogoStyle::Blocks,
        ]
        .into_iter()
        .map(|style| card_body_lines(&theme(), &v, style, 42).len())
        .collect();
        assert!(
            heights.windows(2).all(|w| w[0] == w[1]),
            "welcome body height must not depend on logo style: {heights:?}"
        );
        assert_eq!(
            heights[0],
            logo_region_height() + FIXED_TAIL_LINES as usize
        );
    }
}
