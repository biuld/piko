//! Flex-like composable layout tree (product-agnostic).

use ratatui::layout::{Constraint, Rect};

/// Main-axis direction of a flex container.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Axis {
    /// Top → bottom.
    Column,
    /// Left → right.
    Row,
}

/// Main-axis sizing of one flex item (constrained terminal flex subset).
///
/// - [`Self::Percent`] — share of the **parent flex** main-axis length.
/// - [`Self::Vw`] / [`Self::Vh`] — share of the **root solve area** width/height
///   (viewport-like; independent of nesting depth).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlexSize {
    /// Fixed main-axis length in cells.
    Fixed(u16),
    /// Share of free space along the main axis (`weight` maps to grow factor).
    Grow { weight: u16, min: u16 },
    /// At least `min` cells; may grow if ratatui Min is used alone (maps to Min).
    Min(u16),
    /// `0..=100` percent of the parent flex main-axis length.
    Percent(u16),
    /// `0..=100` percent of the root solve-area **width** (CSS-like `vw`).
    Vw(u16),
    /// `0..=100` percent of the root solve-area **height** (CSS-like `vh`).
    Vh(u16),
}

impl FlexSize {
    /// Map size to a ratatui constraint.
    ///
    /// `root` is the area passed to `solve` / `solve_flex`. `parent_main` is the
    /// parent flex's main-axis length in cells (used only for documentation /
    /// callers that resolve percent manually; percentage is handed to ratatui).
    pub fn to_constraint(self, root: Rect, _parent_main: u16) -> Constraint {
        match self {
            Self::Fixed(n) => Constraint::Length(n),
            Self::Grow { weight, min: _ } => Constraint::Fill(weight.max(1)),
            Self::Min(n) => Constraint::Min(n),
            Self::Percent(p) => Constraint::Percentage(p.min(100)),
            Self::Vw(p) => Constraint::Length(cells_from_percent(root.width, p)),
            Self::Vh(p) => Constraint::Length(cells_from_percent(root.height, p)),
        }
    }
}

/// `percent` of `total`, clamped to `0..=100`, rounded down (terminal cells).
pub fn cells_from_percent(total: u16, percent: u16) -> u16 {
    let p = u32::from(percent.min(100));
    ((u32::from(total) * p) / 100) as u16
}

/// One child of a flex container.
#[derive(Clone, Debug)]
pub struct FlexItem<R> {
    pub size: FlexSize,
    pub child: Node<R>,
}

impl<R> FlexItem<R> {
    pub fn new(size: FlexSize, child: Node<R>) -> Self {
        Self { size, child }
    }

    pub fn fixed(cells: u16, child: Node<R>) -> Self {
        Self::new(FlexSize::Fixed(cells), child)
    }

    pub fn grow(weight: u16, child: Node<R>) -> Self {
        Self::new(FlexSize::Grow { weight, min: 0 }, child)
    }

    /// Percent of **parent** main-axis (0..=100).
    pub fn percent(pct: u16, child: Node<R>) -> Self {
        Self::new(FlexSize::Percent(pct.min(100)), child)
    }

    /// Percent of **root** width (0..=100), applied as main-axis length.
    pub fn vw(pct: u16, child: Node<R>) -> Self {
        Self::new(FlexSize::Vw(pct.min(100)), child)
    }

    /// Percent of **root** height (0..=100), applied as main-axis length.
    pub fn vh(pct: u16, child: Node<R>) -> Self {
        Self::new(FlexSize::Vh(pct.min(100)), child)
    }
}

/// Flex container.
#[derive(Clone, Debug)]
pub struct Flex<R> {
    pub direction: Axis,
    pub children: Vec<FlexItem<R>>,
}

/// Declarative layout tree. `R` is a client-defined region id type.
#[derive(Clone, Debug)]
pub enum Node<R> {
    /// Terminal paint slot; client maps `R` to widgets.
    Leaf(R),
    /// Nested flex container.
    Flex(Flex<R>),
}

impl<R> Node<R> {
    pub fn leaf(region: R) -> Self {
        Self::Leaf(region)
    }

    pub fn flex(direction: Axis, children: Vec<FlexItem<R>>) -> Self {
        Self::Flex(Flex {
            direction,
            children,
        })
    }
}

/// Column flex (vertical).
pub fn flex_column<R>(children: Vec<FlexItem<R>>) -> Node<R> {
    Node::flex(Axis::Column, children)
}

/// Row flex (horizontal).
pub fn flex_row<R>(children: Vec<FlexItem<R>>) -> Node<R> {
    Node::flex(Axis::Row, children)
}

/// Leaf region.
pub fn leaf<R>(region: R) -> Node<R> {
    Node::Leaf(region)
}

/// Alias: vertical stack.
pub fn column<R>(children: Vec<FlexItem<R>>) -> Node<R> {
    flex_column(children)
}

/// Alias: horizontal stack.
pub fn row<R>(children: Vec<FlexItem<R>>) -> Node<R> {
    flex_row(children)
}
