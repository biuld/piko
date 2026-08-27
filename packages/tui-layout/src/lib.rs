//! Generic terminal layout engine for ratatui clients.
//!
//! Product-agnostic: no knowledge of piko sessions, surfaces, or widgets.
//! Downstream crates choose region ids and focus target types via generics.
//!
//! Docs: `docs/features/`, `docs/design/`.

mod content_hit;
mod divider;
mod engine;
mod flex;
mod focus;
mod hitmap;
mod interaction;
mod modal;
mod padding;
mod shell;
mod util;
mod viewport;

pub use content_hit::{
    ContentHitFragment, ContentHitPlan, ContentHitRow, ResolvedContentHit, row_owners,
};
pub use divider::{DividerPlan, DividerSplit, SplitAxis, SplitSize, solve as solve_divider};
pub use engine::{FramePlan, LayerPlan, solve, solve_flex};
pub use flex::{
    Axis, Flex, FlexItem, FlexSize, Node, cells_from_percent, column, flex_column, flex_row, leaf,
    row,
};
pub use focus::FocusManager;
pub use hitmap::{Component, Hit, HitMap, HitRegion, SurfacePanel, build_hitmap};
pub use interaction::{ComponentHit, InteractionState, PointerGesture};
pub use modal::{ModalLayer, ModalPlacement};
pub use padding::{
    Align, Gutter, GutterSide, Padding, Spacer, align, clip, intersection, split_gutter,
};
pub use shell::{ShellChrome, ShellSplit, split_shell};
pub use util::{DEFAULT_HORIZONTAL_INSET, inset_horizontal};
pub use viewport::{
    ScrollbarMetrics, ViewportMetrics, ViewportMode, ViewportPlan, ViewportState, prepare_viewport,
};
