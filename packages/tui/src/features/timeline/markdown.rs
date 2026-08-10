use crate::theme::Theme;
use crate::ui::line_layout::paint_cols;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Parse a markdown string into styled Ratatui Lines using pulldown-cmark.
pub fn parse_markdown(text: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(text, options);
    let mut lines = Vec::new();
    let mut current_line = Vec::new();

    let mut style_stack = Vec::new();
    let mut list_stack = Vec::new(); // next ordered list index or None for unordered
    let mut in_blockquote = false;
    let mut in_code_block = false;
    let mut code_block_buf = String::new();
    let mut code_block_lang = None;

    // GFM table accumulation
    let mut in_table = false;
    let mut in_table_cell = false;
    let mut table_alignments: Vec<Alignment> = Vec::new();
    let mut table_rows: Vec<Vec<TableCell>> = Vec::new();
    let mut table_row: Vec<TableCell> = Vec::new();
    let mut table_cell_spans: Vec<Span<'static>> = Vec::new();
    let mut table_header_rows = 0usize;

    let get_current_style = |stack: &[Style]| -> Style {
        let mut merged = Style::default();
        for s in stack {
            merged = merged.patch(*s);
        }
        merged
    };

    let flush_line = |line: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>| {
        if !line.is_empty() {
            lines.push(Line::from(std::mem::take(line)));
        } else {
            lines.push(Line::from(""));
        }
    };

    let push_cell_text = |cell: &mut Vec<Span<'static>>, text: String, style: Style| {
        if text.is_empty() {
            return;
        }
        cell.push(Span::styled(text, style));
    };

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                }
                Tag::Heading { level, .. } => {
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                    let heading_level_num = match level {
                        pulldown_cmark::HeadingLevel::H1 => 1,
                        pulldown_cmark::HeadingLevel::H2 => 2,
                        pulldown_cmark::HeadingLevel::H3 => 3,
                        pulldown_cmark::HeadingLevel::H4 => 4,
                        pulldown_cmark::HeadingLevel::H5 => 5,
                        pulldown_cmark::HeadingLevel::H6 => 6,
                    };
                    let heading_style = Style::default()
                        .fg(theme.md_heading(heading_level_num))
                        .add_modifier(Modifier::BOLD);
                    style_stack.push(heading_style);

                    let prefix = "#".repeat(heading_level_num as usize) + " ";
                    current_line.push(Span::styled(prefix, heading_style));
                }
                Tag::BlockQuote(_) => {
                    in_blockquote = true;
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    code_block_buf.clear();
                    code_block_lang = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split([',', ' ', '\t'])
                            .next()
                            .filter(|language| !language.is_empty())
                            .map(str::to_string),
                        CodeBlockKind::Indented => None,
                    };
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                    if lines
                        .last()
                        .is_some_and(|line| line.spans.iter().any(|span| !span.content.is_empty()))
                    {
                        lines.push(Line::from(""));
                    }
                }
                Tag::List(start) => {
                    list_stack.push(start);
                }
                Tag::Item => {
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                    let indent_depth = list_stack.len().saturating_sub(1);
                    if indent_depth > 0 {
                        current_line.push(Span::from("  ".repeat(indent_depth)));
                    }

                    let marker_color = theme.md_list_bullet;
                    if let Some(list_type) = list_stack.last_mut() {
                        if let Some(num) = list_type {
                            current_line.push(Span::styled(
                                format!("{}. ", num),
                                Style::default()
                                    .fg(marker_color)
                                    .add_modifier(Modifier::BOLD),
                            ));
                            *num += 1;
                        } else {
                            current_line
                                .push(Span::styled("• ", Style::default().fg(marker_color)));
                        }
                    }
                }
                Tag::Emphasis => {
                    style_stack.push(Style::default().add_modifier(Modifier::ITALIC));
                }
                Tag::Strong => {
                    style_stack.push(Style::default().add_modifier(Modifier::BOLD));
                }
                Tag::Strikethrough => {
                    style_stack.push(Style::default().add_modifier(Modifier::CROSSED_OUT));
                }
                Tag::Link { .. } => {
                    style_stack.push(
                        Style::default()
                            .fg(theme.md_link)
                            .add_modifier(Modifier::UNDERLINED),
                    );
                }
                Tag::Table(alignments) => {
                    in_table = true;
                    table_alignments = alignments;
                    table_rows.clear();
                    table_header_rows = 0;
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                }
                Tag::TableHead => {
                    // Head is a row of cells with **no** `TableRow` wrapper.
                    table_row.clear();
                }
                Tag::TableRow => {
                    table_row.clear();
                }
                Tag::TableCell => {
                    in_table_cell = true;
                    table_cell_spans.clear();
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    flush_line(&mut current_line, &mut lines);
                }
                TagEnd::Heading(_) => {
                    style_stack.pop();
                    flush_line(&mut current_line, &mut lines);
                }
                TagEnd::BlockQuote => {
                    in_blockquote = false;
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    lines.extend(super::highlight::highlight_code_to_lines(
                        &code_block_buf,
                        code_block_lang.take().as_deref(),
                        theme,
                    ));
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                }
                TagEnd::Item => {
                    flush_line(&mut current_line, &mut lines);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    style_stack.pop();
                }
                TagEnd::TableCell => {
                    in_table_cell = false;
                    table_row.push(TableCell::from_spans(std::mem::take(&mut table_cell_spans)));
                }
                TagEnd::TableRow => {
                    // Body rows (header uses TableHead without TableRow).
                    table_rows.push(std::mem::take(&mut table_row));
                }
                TagEnd::TableHead => {
                    if !table_row.is_empty() {
                        table_rows.push(std::mem::take(&mut table_row));
                        table_header_rows = table_rows.len();
                    }
                }
                TagEnd::Table => {
                    in_table = false;
                    lines.extend(render_table(
                        &table_rows,
                        table_header_rows,
                        &table_alignments,
                        theme,
                    ));
                    table_rows.clear();
                    table_alignments.clear();
                    table_header_rows = 0;
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_buf.push_str(&text);
                } else if in_table_cell {
                    let style = get_current_style(&style_stack);
                    push_cell_text(&mut table_cell_spans, text.to_string(), style);
                } else {
                    let mut style = get_current_style(&style_stack);
                    if in_blockquote {
                        style = style.fg(theme.md_quote);
                        if current_line.is_empty() {
                            current_line.push(Span::styled(
                                " > ",
                                Style::default().fg(theme.md_quote_border),
                            ));
                        }
                    }
                    current_line.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                let code_style = Style::default().fg(theme.md_code);
                if in_table_cell {
                    push_cell_text(&mut table_cell_spans, code.to_string(), code_style);
                } else {
                    current_line.push(Span::styled(code.to_string(), code_style));
                }
            }
            Event::SoftBreak => {
                if in_code_block {
                    // preserve inside fence via Text usually; ignore soft
                } else if in_table_cell {
                    push_cell_text(
                        &mut table_cell_spans,
                        " ".into(),
                        get_current_style(&style_stack),
                    );
                } else {
                    current_line.push(Span::from(" "));
                }
            }
            Event::HardBreak => {
                if in_table_cell {
                    push_cell_text(
                        &mut table_cell_spans,
                        " ".into(),
                        get_current_style(&style_stack),
                    );
                } else if !in_code_block {
                    flush_line(&mut current_line, &mut lines);
                }
            }
            _ => {
                let _ = in_table; // silence when only nested tags move state
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}

#[derive(Clone, Default)]
struct TableCell {
    spans: Vec<Span<'static>>,
    width: usize,
}

impl TableCell {
    fn from_spans(spans: Vec<Span<'static>>) -> Self {
        let width = spans.iter().map(|s| paint_cols(s.content.as_ref())).sum();
        Self { spans, width }
    }

    fn plain(&self) -> String {
        self.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }
}

/// Terminal-friendly table: padded columns, header rule, no raw pipe soup.
fn render_table(
    rows: &[Vec<TableCell>],
    header_rows: usize,
    alignments: &[Alignment],
    theme: &Theme,
) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return Vec::new();
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }

    let mut widths = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width).max(1);
        }
    }
    // Cap very wide columns so the stream stays scannable.
    const MAX_COL: usize = 40;
    for w in &mut widths {
        *w = (*w).min(MAX_COL);
    }

    let rule_style = Style::default().fg(theme.dim);
    let sep = "  ";
    let mut out = Vec::new();

    for (row_idx, row) in rows.iter().enumerate() {
        let is_header = row_idx < header_rows;
        let mut spans = Vec::new();
        for (col, target) in widths.iter().copied().enumerate() {
            if col > 0 {
                spans.push(Span::styled(sep.to_string(), rule_style));
            }
            let cell = row.get(col);
            let align = alignments.get(col).copied().unwrap_or(Alignment::Left);
            let (cell_spans, cell_w) = match cell {
                Some(c) => (c.spans.clone(), c.width.min(target)),
                None => (Vec::new(), 0),
            };
            // Truncate oversized cells by plain width for layout simplicity.
            let mut painted = if cell.map(|c| c.width).unwrap_or(0) > target {
                let plain = cell.map(TableCell::plain).unwrap_or_default();
                let clipped = crate::ui::line_layout::truncate_paint_cols(&plain, target);
                let style = if is_header {
                    Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text)
                };
                vec![Span::styled(clipped, style)]
            } else if is_header {
                cell_spans
                    .into_iter()
                    .map(|s| {
                        Span::styled(
                            s.content.to_string(),
                            s.style.add_modifier(Modifier::BOLD).fg(theme.text),
                        )
                    })
                    .collect()
            } else {
                cell_spans
            };
            let pad = target.saturating_sub(cell_w);
            let (left_pad, right_pad) = match align {
                Alignment::Center => (pad / 2, pad - pad / 2),
                Alignment::Right => (pad, 0),
                _ => (0, pad),
            };
            if left_pad > 0 {
                spans.push(Span::from(" ".repeat(left_pad)));
            }
            spans.append(&mut painted);
            if right_pad > 0 {
                spans.push(Span::from(" ".repeat(right_pad)));
            }
        }
        out.push(Line::from(spans));

        if is_header && row_idx + 1 == header_rows {
            // Header rule under the last header row.
            let mut rule = Vec::new();
            for (col, w) in widths.iter().enumerate() {
                if col > 0 {
                    rule.push(Span::styled(sep.to_string(), rule_style));
                }
                rule.push(Span::styled("─".repeat(*w), rule_style));
            }
            out.push(Line::from(rule));
        }
    }
    out.push(Line::from(""));
    out
}

#[cfg(test)]
#[path = "markdown_tests.rs"]
mod tests;
