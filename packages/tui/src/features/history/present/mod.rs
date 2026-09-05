//! Scan-row and typed-detail presentation for Session History.

mod detail;
mod labels;
mod paint;
mod rows;

#[cfg(test)]
mod tests;

pub(crate) use detail::detail_lines;
pub(crate) use rows::{empty_copy, row_line};
