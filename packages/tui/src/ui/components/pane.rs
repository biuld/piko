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

use ratatui::{layout::Rect, style::Color, text::Line, widgets::Borders};

use crate::theme::Theme;
use crate::ui::components::feedback::frame_border_style;
use crate::ui::interaction_hints::InteractionHints;

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
    Hints(InteractionHints<'a>),
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
    /// Optional backdrop fill behind the chrome (opaque floating modals).
    /// Dock bands leave this `None` so they match the shell background.
    #[allow(dead_code)] // public Pane API; Decide docks intentionally omit fill
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

    pub fn hints(mut self, hints: impl Into<InteractionHints<'a>>) -> Self {
        let hints = hints.into();
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
    #[allow(dead_code)] // public Pane API; Decide docks intentionally omit fill
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }

    /// The content zone [`render_pane`] paints into for this area — pure
    /// geometry shared by renderers and hit-testing so they cannot drift.
    pub fn content_rect(&self, area: Rect) -> Option<Rect> {
        render::prepare_pane(area, self).map(|plan| plan.content)
    }

    /// Reserved footer geometry, derived without painting.
    pub fn footer_rect(&self, area: Rect) -> Option<Rect> {
        matches!(self.footer, PaneFooter::Reserved { .. })
            .then(|| render::prepare_pane(area, self).and_then(|plan| plan.footer))
            .flatten()
    }

    /// Search/custom-input row geometry, derived without painting.
    pub fn search_rect(&self, area: Rect) -> Option<Rect> {
        render::prepare_pane(area, self).and_then(|plan| plan.search)
    }

    /// Clickable title-affix geometry. Selection counters and labels remain
    /// informational; close and mode options expose semantic targets.
    pub fn title_affix_regions(&self, area: Rect) -> Vec<(Rect, PaneAffixHit)> {
        render::prepare_pane(area, self)
            .map(|plan| plan.affix_hits)
            .unwrap_or_default()
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

/// Zones after chrome is painted.
mod render;
#[allow(unused_imports)]
use render::footer_height;
#[allow(unused_imports)]
pub use render::{
    PaneAreas, PanePlan, format_title_affixes, paint_pane, prepare_pane, render_pane,
    section_rule_line,
};

#[cfg(test)]
mod tests;
