//! Native semantic Markdown parsing and GPUI rendering.

mod model;
mod parse;
mod render;

pub use model::MarkdownDocument;
pub use parse::parse_markdown;
pub use render::render_markdown;

#[cfg(test)]
mod tests;
