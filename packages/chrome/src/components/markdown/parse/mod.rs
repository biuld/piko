//! Pulldown-cmark adapter and fail-soft parser entry point.

mod builder;
mod frame;

use pulldown_cmark::{Options, Parser};

use super::model::MarkdownDocument;
use builder::Builder;

pub fn parse_markdown(source: &str) -> MarkdownDocument {
    parse(source).unwrap_or_else(|_| MarkdownDocument::plain(source))
}

fn parse(source: &str) -> Result<MarkdownDocument, ParseError> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut builder = Builder::default();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        builder.event(event, range)?;
    }
    builder.finish()
}

#[derive(Debug)]
pub(super) struct ParseError;
