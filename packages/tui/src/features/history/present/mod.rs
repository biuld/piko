//! Scan-row and typed-detail presentation for Session History.

mod content;
mod context;
mod detail;
mod labels;
mod paint;
mod rows;

#[cfg(test)]
mod tests;

pub(crate) use context::row_context;
pub(crate) use detail::detail_lines;
pub(crate) use rows::{empty_copy, row_line};

pub(super) use paint::wrapped as feedback_lines;
