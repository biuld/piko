//! Scan-row and typed-detail presentation for Session Trajectory.

mod content;
mod context;
mod detail;
mod labels;
mod paint;
mod rows;
mod tool;

#[cfg(test)]
mod tests;

pub(crate) use detail::tab_lines;
pub(crate) use rows::{empty_copy, row_line};

pub(super) use paint::wrapped as feedback_lines;
