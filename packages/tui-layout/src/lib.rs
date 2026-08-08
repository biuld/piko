//! Generic terminal layout engine for ratatui clients.
//!
//! Product-agnostic: no knowledge of piko sessions, surfaces, or widgets.
//! Downstream crates choose region ids and focus target types via generics.
//!
//! Docs: `docs/features/`, `docs/design/`.

mod engine;
mod flex;
mod focus;
mod hitmap;
mod modal;
mod shell;
mod util;

pub use engine::{FramePlan, LayerPlan, solve, solve_flex};
pub use flex::{
    Axis, Flex, FlexItem, FlexSize, Node, cells_from_percent, column, flex_column, flex_row, leaf,
    row,
};
pub use focus::FocusManager;
pub use hitmap::{Component, Hit, HitMap, HitRegion, SurfacePanel, build_hitmap};
pub use modal::{ModalLayer, ModalPlacement};
pub use shell::{ShellChrome, ShellSplit, split_shell};
pub use util::{DEFAULT_HORIZONTAL_INSET, inset_horizontal};
