use crate::theme::Theme;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
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
    let mut list_stack = Vec::new(); // tracks next ordered list index or None for unordered
    let mut in_blockquote = false;
    let mut in_code_block = false;
    let mut code_block_buf = String::new();

    // helper to get the active merged style from stack
    let get_current_style = |stack: &[Style]| -> Style {
        let mut merged = Style::default();
        for s in stack {
            merged = merged.patch(*s);
        }
        merged
    };

    // helper to flush current line
    let flush_line = |line: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>| {
        if !line.is_empty() {
            lines.push(Line::from(std::mem::take(line)));
        } else {
            lines.push(Line::from(""));
        }
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
                    let heading_style = Style::default()
                        .fg(theme.get("mdHeading"))
                        .add_modifier(Modifier::BOLD);
                    style_stack.push(heading_style);

                    let heading_level_num = match level {
                        pulldown_cmark::HeadingLevel::H1 => 1,
                        pulldown_cmark::HeadingLevel::H2 => 2,
                        pulldown_cmark::HeadingLevel::H3 => 3,
                        pulldown_cmark::HeadingLevel::H4 => 4,
                        pulldown_cmark::HeadingLevel::H5 => 5,
                        pulldown_cmark::HeadingLevel::H6 => 6,
                    };
                    let prefix = "#".repeat(heading_level_num) + " ";
                    current_line.push(Span::styled(prefix, heading_style));
                }
                Tag::BlockQuote(_) => {
                    in_blockquote = true;
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
                    }
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    code_block_buf.clear();
                    if !current_line.is_empty() {
                        flush_line(&mut current_line, &mut lines);
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

                    let marker_color = theme.get("mdListBullet");
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
                            .fg(theme.get("mdLink"))
                            .add_modifier(Modifier::UNDERLINED),
                    );
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
                    let code_color = theme.get("mdCodeBlock");
                    let border_color = theme.get("mdCodeBlockBorder");

                    lines.push(Line::from(Span::styled(
                        " ┌─── Code Block ────────────────────────────",
                        Style::default().fg(border_color),
                    )));

                    for line in code_block_buf.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(" │ ", Style::default().fg(border_color)),
                            Span::styled(format!(" {}", line), Style::default().fg(code_color)),
                        ]));
                    }

                    lines.push(Line::from(Span::styled(
                        " └───────────────────────────────────────────",
                        Style::default().fg(border_color),
                    )));
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
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    code_block_buf.push_str(&text);
                } else {
                    let mut style = get_current_style(&style_stack);
                    if in_blockquote {
                        style = style.fg(theme.get("mdQuote"));
                        if current_line.is_empty() {
                            current_line.push(Span::styled(
                                " > ",
                                Style::default().fg(theme.get("mdQuoteBorder")),
                            ));
                        }
                    }
                    current_line.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                let code_style = Style::default().fg(theme.get("mdCode"));
                current_line.push(Span::styled(code.to_string(), code_style));
            }
            Event::SoftBreak => {
                if !in_code_block {
                    current_line.push(Span::from(" "));
                }
            }
            Event::HardBreak => {
                flush_line(&mut current_line, &mut lines);
            }
            _ => {}
        }
    }

    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    lines
}
