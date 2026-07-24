//! Row-scoped selectable rich text and semantic clipboard support.

mod model;
mod region;
mod text;

pub use model::{SelectionGroup, SelectionState};
pub use region::{CopySelection, selectable_region};
pub use text::SelectableText;

pub(crate) use region::init;
