//! Shared terminal-column text preparation for paint, hit testing, and editor
//! pointer placement.

pub mod model;
pub mod position;
pub mod wrap;

#[allow(unused_imports)]
pub use model::{
    Breakability, PositionBias, TextLayout, TextRun, VisualFragment, VisualLine, VisualPosition,
};
#[allow(unused_imports)]
pub use wrap::{fragment_source, to_lines, wrap_runs, wrap_source, wrap_spans};
