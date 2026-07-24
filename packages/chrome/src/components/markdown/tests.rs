use super::model::{InlineStyle, MarkdownBlock, MarkdownDocument, MarkdownInline};
use super::parse::parse_markdown;

fn block(document: &MarkdownDocument, index: usize) -> &MarkdownBlock {
    &document.blocks[index].value
}

#[test]
fn parses_nested_list_continuation_as_vertical_item_blocks() {
    let source =
        "1. first paragraph\n\n   continuation paragraph\n\n   - nested item\n2. second item\n";
    let document = parse_markdown(source);
    let MarkdownBlock::List(list) = block(&document, 0) else {
        panic!("expected list");
    };
    assert_eq!(list.start, Some(1));
    assert_eq!(list.items.len(), 2);
    assert_eq!(list.items[0].blocks.len(), 3);
    assert!(matches!(
        list.items[0].blocks[0].value,
        MarkdownBlock::Paragraph(_)
    ));
    assert!(matches!(
        list.items[0].blocks[1].value,
        MarkdownBlock::Paragraph(_)
    ));
    assert!(matches!(
        list.items[0].blocks[2].value,
        MarkdownBlock::List(_)
    ));
}

#[test]
fn parses_table_header_and_rows() {
    let document = parse_markdown("| left | right |\n| :--- | ---: |\n| a | b |\n");
    let MarkdownBlock::Table(table) = block(&document, 0) else {
        panic!("expected table");
    };
    assert_eq!(table.alignments.len(), 2);
    assert_eq!(table.head.len(), 2);
    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0].len(), 2);
}

#[test]
fn preserves_raw_html_as_literal_text() {
    let document = parse_markdown("<script>alert('no')</script>\n");
    let MarkdownBlock::Paragraph(content) = block(&document, 0) else {
        panic!("expected literal paragraph");
    };
    assert_eq!(
        content,
        &[MarkdownInline::Text(
            "<script>alert('no')</script>\n".into()
        )]
    );
}

#[test]
fn unterminated_code_fence_remains_code() {
    let document = parse_markdown("```rust\nfn main() {}");
    let MarkdownBlock::CodeBlock { language, code } = block(&document, 0) else {
        panic!("expected code block");
    };
    assert_eq!(language.as_deref(), Some("rust"));
    assert_eq!(code, "fn main() {}");
}

#[test]
fn empty_input_is_an_empty_document() {
    assert!(parse_markdown("").blocks.is_empty());
}

#[test]
fn parses_inline_styles_links_images_and_task_markers() {
    let document = parse_markdown(
        "- [x] **strong** and _emphasis_ with [link](https://example.com) ![alt](image.png)\n",
    );
    let MarkdownBlock::List(list) = block(&document, 0) else {
        panic!("expected list");
    };
    assert_eq!(list.items[0].checked, Some(true));
    let MarkdownBlock::Paragraph(content) = &list.items[0].blocks[0].value else {
        panic!("expected paragraph");
    };
    assert!(content.iter().any(|inline| matches!(
        inline,
        MarkdownInline::Styled {
            kind: InlineStyle::Strong,
            ..
        }
    )));
    assert!(content.iter().any(|inline| matches!(
        inline,
        MarkdownInline::Link { destination, .. } if destination == "https://example.com"
    )));
    assert!(
        content
            .iter()
            .any(|inline| matches!(inline, MarkdownInline::ImageAlt(_)))
    );
}

#[test]
fn excessive_nesting_falls_back_without_losing_source() {
    let source = format!("{}text", "> ".repeat(140));
    let document = parse_markdown(&source);
    assert_eq!(document.blocks.len(), 1);
    let MarkdownBlock::Paragraph(content) = block(&document, 0) else {
        panic!("expected plain fallback");
    };
    assert_eq!(content, &[MarkdownInline::Text(source)]);
}
