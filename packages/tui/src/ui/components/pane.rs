//! Pane — reusable framed chrome for product surfaces.
//!
//! Feature logic lives outside Pane. Pane owns frame + vertical zones and
//! exposes a **complexity mode** plus field overrides. Mode describes how
//! rich the *pane chrome* should be for that surface’s job — not layout
//! placement (`CoverBody` / `ComposerBand` live in navigation).
//!
//! Standard (default surfaces: settings, sessions, long-form info):
//! ```text
//! ┌─ Title                                 [x] ─┐
//! │ / type to filter                             │
//! │ ─────────────────────────────────────────── │
//! │ Content…                                    │
//! │ Tip · …                                     │
//! │ ↑/↓ nav | Enter open | Esc close            │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! Minimal (quick pickers / simple prompts — fewer zones by default):
//! ```text
//! ─ agents ────────────────────────── 1/3 ─
//! / query
//! ❯ ○ main                   current
//!   ○ coder                   idle
//! ↑/↓ | Enter switch | Esc
//! ────────────────────────────────────────
//! ```

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::theme::Theme;
use crate::ui::components::feedback::frame_border_style;

/// Search-row glyph (product convention: command-style `/` affordance).
pub const SEARCH_GLYPH: &str = "/";

/// Default search placeholder shown while the filter is empty.
pub const SEARCH_PLACEHOLDER: &str = "type to filter";

/// Pane chrome **complexity** for a surface’s job.
///
/// This is product/chrome density, not modal placement. A full session browser
/// and a long-form info panel both use [`Standard`]; a viewed-agent switcher or
/// model picker uses [`Minimal`] because the interaction is a short pick.
///
/// Applying a mode via [`PaneSpec::mode`] sets padding / borders / search-rule
/// defaults; subsequent builders (`.padding`, `.borders`, `.search_rule`, …)
/// override field-by-field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneMode {
    /// Default chrome: full frame, roomy pad, search hairline when search is on.
    #[default]
    Standard,
    /// Sparse chrome: top/bottom frame, tight vertical pad, no search hairline.
    Minimal,
}

/// Inner content inset inside the block border (cells).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanePadding {
    pub horizontal: u16,
    pub vertical: u16,
}

impl PanePadding {
    pub const fn new(horizontal: u16, vertical: u16) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    pub const UNIFORM_1: Self = Self::new(1, 1);
}

impl PaneMode {
    /// Default inner padding for this complexity.
    pub const fn padding(self) -> PanePadding {
        match self {
            Self::Standard => PanePadding::UNIFORM_1,
            // Horizontal gutters only — keep list rows dense.
            Self::Minimal => PanePadding::new(1, 0),
        }
    }

    /// Default outer border set for this complexity.
    pub const fn borders(self) -> Borders {
        match self {
            Self::Standard => Borders::ALL,
            Self::Minimal => Borders::TOP.union(Borders::BOTTOM),
        }
    }

    /// Default: hairline under the search row.
    pub const fn search_rule(self) -> bool {
        match self {
            Self::Standard => true,
            Self::Minimal => false,
        }
    }
}

/// Whether the pane reserves a search line under the title.
#[derive(Clone, Debug, Default)]
pub enum PaneSearch<'a> {
    /// No search row.
    #[default]
    Hidden,
    /// Placeholder when empty; live filter text when non-empty.
    Shown {
        filter: &'a str,
        /// Text after the glyph when empty; defaults to [`SEARCH_PLACEHOLDER`].
        placeholder: Option<&'a str>,
    },
    /// Fully custom search / prompt line (label editor, scoped filters, …).
    Custom(Line<'a>),
}

/// Footer zone under content (and optional tip).
#[derive(Clone, Copy, Debug, Default)]
pub enum PaneFooter<'a> {
    #[default]
    None,
    /// Dim binding legend; multi-line if `text` contains `\n`.
    Hints(&'a str),
    /// Reserve `height` rows; caller paints into [`PaneAreas::footer`].
    Reserved { height: u16 },
}

/// One mutually-exclusive **mode strip** in the title (scope, filter, …).
///
/// Feature owns *which* option is active; Pane owns paint:
/// `Default | [NoTools] | User | …` (active option in brackets, ` | ` between).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneModeStrip {
    pub options: Vec<String>,
    /// Index into [`options`]; clamped when painted if out of range.
    pub active: usize,
}

impl PaneModeStrip {
    pub fn new(options: impl IntoIterator<Item = impl Into<String>>, active: usize) -> Self {
        let options: Vec<String> = options.into_iter().map(Into::into).collect();
        Self { options, active }
    }

    fn clamped_active(&self) -> usize {
        if self.options.is_empty() {
            0
        } else {
            self.active.min(self.options.len() - 1)
        }
    }

    /// Product string for this strip alone.
    pub fn display(&self) -> String {
        if self.options.is_empty() {
            return String::new();
        }
        let active = self.clamped_active();
        self.options
            .iter()
            .enumerate()
            .map(|(i, label)| {
                if i == active {
                    format!("[{label}]")
                } else {
                    label.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Semantic chips on the **right side of the title bar**.
///
/// Callers declare meaning; Pane owns formatting and spacing (affixes are
/// painted left → right within the right-aligned cluster).
///
/// ```text
/// ┌─ Title          Current | [All]  [3/12]  [x] ─┐
///                   └─ ModeStrip ─┘  └─ sel ─┘  Close
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneTitleAffix {
    /// Esc-close affordance → `[x]`.
    Close,
    /// Free-form chip (e.g. `tool: bash`).
    Label(String),
    /// List/table selection counter → `[at/of]` (1-based `at`).
    Selection { at: usize, of: usize },
    /// Mutually-exclusive options; Pane highlights the active one.
    ModeStrip(PaneModeStrip),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneAffixHit {
    Close,
    ModeOption(usize),
}

impl PaneTitleAffix {
    pub fn label(label: impl Into<String>) -> Self {
        Self::Label(label.into())
    }

    pub fn mode_strip(options: impl IntoIterator<Item = impl Into<String>>, active: usize) -> Self {
        Self::ModeStrip(PaneModeStrip::new(options, active))
    }

    pub fn mode_strip_static(options: &[&'static str], active: usize) -> Self {
        Self::mode_strip(options.iter().copied(), active)
    }

    pub fn selection(at_one_based: usize, of: usize) -> Self {
        Self::Selection {
            at: at_one_based,
            of,
        }
    }

    /// Product string for this affix alone.
    pub fn display(&self) -> String {
        match self {
            Self::Close => "[x]".to_string(),
            Self::Label(label) => label.clone(),
            Self::Selection { at, of } => format!("[{at}/{of}]"),
            Self::ModeStrip(strip) => strip.display(),
        }
    }
}

/// Join title affixes for the right title segment (double-space separation).
pub fn format_title_affixes(affixes: &[PaneTitleAffix]) -> String {
    affixes
        .iter()
        .map(PaneTitleAffix::display)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ")
}

/// Spec for chrome paint. Content is filled by the caller after layout.
#[derive(Clone, Debug)]
pub struct PaneSpec<'a> {
    pub title: &'a str,
    /// Right-aligned title chips (mode · selection · close, …).
    pub title_affixes: Vec<PaneTitleAffix>,
    pub mode: PaneMode,
    pub padding: PanePadding,
    pub borders: Borders,
    pub search: PaneSearch<'a>,
    /// Hairline rule under the search row (on when search is visible, unless cleared).
    pub search_rule: bool,
    pub footer: PaneFooter<'a>,
    /// Optional tip line above the footer.
    pub tip: Option<&'a str>,
    /// Optional backdrop fill behind the chrome (opaque modal dialogs).
    pub fill: Option<Color>,
    pub focused: bool,
}

impl<'a> PaneSpec<'a> {
    /// Standard-complexity pane with product defaults.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            title_affixes: Vec::new(),
            mode: PaneMode::Standard,
            padding: PaneMode::Standard.padding(),
            borders: PaneMode::Standard.borders(),
            search: PaneSearch::Hidden,
            search_rule: PaneMode::Standard.search_rule(),
            footer: PaneFooter::None,
            tip: None,
            fill: None,
            focused: true,
        }
    }

    /// Minimal-complexity defaults (quick pickers / short prompts).
    pub fn minimal(title: &'a str) -> Self {
        Self::new(title).mode(PaneMode::Minimal)
    }

    /// Apply a complexity preset. Resets mode-owned defaults (padding, borders,
    /// search_rule); other fields stay; callers re-apply overrides after.
    pub fn mode(mut self, mode: PaneMode) -> Self {
        self.mode = mode;
        self.padding = mode.padding();
        self.borders = mode.borders();
        self.search_rule = mode.search_rule();
        self
    }

    pub fn padding(mut self, padding: PanePadding) -> Self {
        self.padding = padding;
        self
    }

    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    /// Replace the full right-title affix list (left → right within the cluster).
    pub fn title_affixes(mut self, affixes: impl IntoIterator<Item = PaneTitleAffix>) -> Self {
        self.title_affixes = affixes.into_iter().collect();
        self
    }

    /// Append one affix (e.g. counter then close).
    pub fn affix(mut self, affix: PaneTitleAffix) -> Self {
        self.title_affixes.push(affix);
        self
    }

    pub fn search_filter(mut self, filter: &'a str) -> Self {
        self.search = PaneSearch::Shown {
            filter,
            placeholder: None,
        };
        self
    }

    pub fn search(mut self, search: PaneSearch<'a>) -> Self {
        self.search = search;
        self
    }

    /// Feature opts out of the search zone (filter lives elsewhere, e.g. editor
    /// token for slash suggestions / file browser).
    pub fn no_search(mut self) -> Self {
        self.search = PaneSearch::Hidden;
        self.search_rule = false;
        self
    }

    pub fn search_rule(mut self, on: bool) -> Self {
        self.search_rule = on;
        self
    }

    pub fn hints(mut self, hints: &'a str) -> Self {
        self.footer = if hints.is_empty() {
            PaneFooter::None
        } else {
            PaneFooter::Hints(hints)
        };
        self
    }

    pub fn footer(mut self, footer: PaneFooter<'a>) -> Self {
        self.footer = footer;
        self
    }

    pub fn tip(mut self, tip: impl Into<Option<&'a str>>) -> Self {
        self.tip = tip.into();
        self
    }

    /// Fill the whole area with `color` behind the chrome (modal backdrop).
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// The content zone [`render_pane`] paints into for this area — pure
    /// geometry shared by renderers and hit-testing so they cannot drift.
    pub fn content_rect(&self, area: Rect) -> Option<Rect> {
        let bordered = Block::default().borders(self.borders).inner(area);
        let inner = inset_xy(bordered, self.padding);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        let footer_h = footer_height(self.footer);
        let show_search = !matches!(self.search, PaneSearch::Hidden);
        let show_rule = show_search && self.search_rule;
        let show_tip = self.tip.is_some_and(|t| !t.is_empty());
        let chrome = u16::from(show_search)
            .saturating_add(u16::from(show_rule))
            .saturating_add(u16::from(show_tip))
            .saturating_add(footer_h);
        if inner.height <= chrome {
            // Mirrors render_pane's fallback: content is inner minus footer
            // when a footer fits, otherwise the whole inner.
            if footer_h > 0 && inner.height > footer_h {
                return Some(Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height - footer_h,
                ));
            }
            return Some(inner);
        }
        Some(Rect::new(
            inner.x,
            inner
                .y
                .saturating_add(u16::from(show_search))
                .saturating_add(u16::from(show_rule)),
            inner.width,
            inner.height - chrome,
        ))
    }

    /// Reserved footer geometry, derived without painting.
    pub fn footer_rect(&self, area: Rect) -> Option<Rect> {
        let PaneFooter::Reserved { .. } = self.footer else {
            return None;
        };
        let bordered = Block::default().borders(self.borders).inner(area);
        let inner = inset_xy(bordered, self.padding);
        let height = footer_height(self.footer).min(inner.height);
        (height > 0).then_some(Rect::new(
            inner.x,
            inner.y.saturating_add(inner.height.saturating_sub(height)),
            inner.width,
            height,
        ))
    }

    /// Search/custom-input row geometry, derived without painting.
    pub fn search_rect(&self, area: Rect) -> Option<Rect> {
        if matches!(self.search, PaneSearch::Hidden) {
            return None;
        }
        let bordered = Block::default().borders(self.borders).inner(area);
        let inner = inset_xy(bordered, self.padding);
        (inner.width > 0 && inner.height > 0).then_some(Rect::new(inner.x, inner.y, inner.width, 1))
    }

    /// Clickable title-affix geometry. Selection counters and labels remain
    /// informational; close and mode options expose semantic targets.
    pub fn title_affix_regions(&self, area: Rect) -> Vec<(Rect, PaneAffixHit)> {
        if self.title_affixes.is_empty() || area.width < 3 || area.height == 0 {
            return Vec::new();
        }
        let displays: Vec<String> = self
            .title_affixes
            .iter()
            .map(PaneTitleAffix::display)
            .collect();
        let cluster = displays.join("  ");
        let line_width = cluster.chars().count() as u16 + 2;
        let start = area
            .x
            .saturating_add(area.width.saturating_sub(1).saturating_sub(line_width));
        let mut x = start.saturating_add(1);
        let mut out = Vec::new();
        for (affix, display) in self.title_affixes.iter().zip(displays) {
            match affix {
                PaneTitleAffix::Close => out.push((
                    Rect::new(x, area.y, display.chars().count() as u16, 1),
                    PaneAffixHit::Close,
                )),
                PaneTitleAffix::ModeStrip(strip) => {
                    let mut option_x = x;
                    for (index, option) in strip.options.iter().enumerate() {
                        let width = option.chars().count() as u16
                            + if index == strip.clamped_active() {
                                2
                            } else {
                                0
                            };
                        out.push((
                            Rect::new(option_x, area.y, width, 1),
                            PaneAffixHit::ModeOption(index),
                        ));
                        option_x = option_x.saturating_add(width).saturating_add(3); // " | "
                    }
                }
                PaneTitleAffix::Label(_) | PaneTitleAffix::Selection { .. } => {}
            }
            x = x
                .saturating_add(display.chars().count() as u16)
                .saturating_add(2);
        }
        out
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

/// Zones after chrome is painted.
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
        .border_style(frame_border_style(spec.focused, theme))
        .title(
            Line::from(Span::styled(
                format!(" {} ", spec.title),
                Style::default().fg(theme.text),
            ))
            .alignment(Alignment::Left),
        );
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

fn inset_xy(area: Rect, pad: PanePadding) -> Rect {
    let hx = pad.horizontal.min(area.width.saturating_sub(1) / 2);
    let vy = pad.vertical.min(area.height.saturating_sub(1) / 2);
    Rect {
        x: area.x.saturating_add(hx),
        y: area.y.saturating_add(vy),
        width: area.width.saturating_sub(hx.saturating_mul(2)),
        height: area.height.saturating_sub(vy.saturating_mul(2)),
    }
}

fn footer_height(footer: PaneFooter<'_>) -> u16 {
    match footer {
        PaneFooter::None => 0,
        PaneFooter::Hints("") => 0,
        PaneFooter::Hints(_) => 1,
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

fn paint_hints(frame: &mut Frame<'_>, area: Rect, hints: &str, theme: &Theme) {
    let Some(hint) = hints.lines().find(|line| !line.is_empty()) else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_rule_contains_label() {
        let theme = Theme::dark();
        let line = section_rule_line("Appearance", 40, &theme);
        let joined: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.starts_with("Appearance"));
        assert!(joined.contains('─'));
    }

    #[test]
    fn pane_spec_builder() {
        let s = PaneSpec::new("Settings")
            .affix(PaneTitleAffix::Close)
            .search_filter("")
            .hints("Esc close")
            .focused(true);
        assert_eq!(s.title, "Settings");
        assert!(matches!(s.search, PaneSearch::Shown { filter: "", .. }));
        assert!(matches!(s.footer, PaneFooter::Hints("Esc close")));
        assert!(s.search_rule);
        assert_eq!(s.mode, PaneMode::Standard);
        assert_eq!(s.padding, PanePadding::UNIFORM_1);
        assert_eq!(s.title_affixes, vec![PaneTitleAffix::Close]);
    }

    #[test]
    fn title_affixes_paint_semantics() {
        assert_eq!(PaneTitleAffix::Close.display(), "[x]");
        assert_eq!(PaneTitleAffix::label("tool: bash").display(), "tool: bash");
        assert_eq!(PaneTitleAffix::selection(2, 9).display(), "[2/9]");
        assert_eq!(
            PaneModeStrip::new(["Current", "All"], 1).display(),
            "Current | [All]"
        );
        assert_eq!(
            PaneModeStrip::new(["Default", "NoTools", "User", "Labeled", "All"], 1).display(),
            "Default | [NoTools] | User | Labeled | All"
        );
        assert_eq!(
            format_title_affixes(&[
                PaneTitleAffix::mode_strip(["Current", "All"], 0),
                PaneTitleAffix::selection(1, 3),
                PaneTitleAffix::Close,
            ]),
            "[Current] | All  [1/3]  [x]"
        );
    }

    #[test]
    fn content_rect_matches_manual_geometry() {
        let area = Rect::new(0, 0, 60, 12);

        // Minimal: borders TOP|BOTTOM → (0,1,60,10); padding (1,0) →
        // (1,1,58,10); one-row hints footer → (1,1,58,9).
        let spec = PaneSpec::minimal("t").hints("help");
        assert_eq!(spec.content_rect(area), Some(Rect::new(1, 1, 58, 9)));

        // Standard: borders ALL → (1,1,58,10); padding (1,1) → (2,2,56,8);
        // one-row hints footer → (2,2,56,7).
        let spec = PaneSpec::new("t").hints("help");
        assert_eq!(spec.content_rect(area), Some(Rect::new(2, 2, 56, 7)));

        // Standard + search + rule + tip + reserved footer: content starts
        // below the first two zones and chrome consumes 5 rows in total.
        let spec = PaneSpec::new("t")
            .search_filter("x")
            .tip("tip")
            .footer(PaneFooter::Reserved { height: 2 });
        assert_eq!(spec.content_rect(area), Some(Rect::new(2, 4, 56, 3)));

        // Too small for any chrome: no content.
        assert_eq!(
            PaneSpec::new("t")
                .hints("h")
                .content_rect(Rect::new(0, 0, 60, 2)),
            None
        );
    }

    #[test]
    fn mode_strip_clamps_active() {
        let strip = PaneModeStrip::new(["A", "B"], 99);
        assert_eq!(strip.display(), "A | [B]");
        assert_eq!(PaneModeStrip::new(Vec::<String>::new(), 0).display(), "");
    }

    #[test]
    fn minimal_mode_defaults_sparse_chrome() {
        let s = PaneSpec::minimal("agents").search_filter("").hints("Esc");
        assert_eq!(s.mode, PaneMode::Minimal);
        assert_eq!(s.padding, PanePadding::new(1, 0));
        assert!(!s.search_rule);
        assert_eq!(s.borders, Borders::TOP.union(Borders::BOTTOM));
        // Explicit override wins after mode.
        let s = s.search_rule(true).padding(PanePadding::new(0, 0));
        assert!(s.search_rule);
        assert_eq!(s.padding, PanePadding::new(0, 0));
        let s = s.borders(Borders::ALL);
        assert_eq!(s.borders, Borders::ALL);
    }

    #[test]
    fn no_search_hides_filter_zone() {
        let s = PaneSpec::minimal("slash suggestions")
            .search_filter("query")
            .no_search()
            .hints("Tab");
        assert!(matches!(s.search, PaneSearch::Hidden));
        assert!(!s.search_rule);
    }

    #[test]
    fn footer_height_is_one_for_any_non_empty_hint() {
        assert_eq!(footer_height(PaneFooter::Hints("a\nb")), 1);
        assert_eq!(footer_height(PaneFooter::Hints("a")), 1);
        assert_eq!(footer_height(PaneFooter::Hints("")), 0);
    }
}
