# Native Markdown Renderer Design

## Pipeline

```text
Markdown source
    → pulldown-cmark events with source offsets
    → owned semantic document
    → block and inline dispatch
    → ordinary GPUI elements
```

The parser adapter and renderer are separate. Parser-library types do not leak
into the rendering API, and GPUI types do not enter the semantic model. This
keeps parser fixtures deterministic and allows rendering to evolve without
changing source interpretation.

The public entry point is `components::markdown`. It exposes only
`MarkdownDocument`, `parse_markdown`, and `render_markdown`; block and inline
types remain private so consumers cannot couple themselves to parser internals.

## Semantic model

The document owns source-ranged top-level and nested blocks. Block variants
cover paragraphs, headings, block quotes, lists, code blocks, tables, and
thematic breaks. Inline variants cover text, nested styles, code, links, image
alternative text, and soft or hard breaks.

Lists own items, and every item owns a vector of blocks. The marker is not part
of the item body. A continuation paragraph or nested list therefore becomes a
vertical child rather than another child in the marker's horizontal row.

## Parsing

The adapter consumes balanced start/end events with an explicit frame stack.
It enables tables, strikethrough, and task-list markers. Tight-list text that
does not receive an explicit paragraph event is wrapped in an implicit
paragraph belonging to the current item.

Raw HTML becomes literal text. Images discard their source and retain their
alternative content. Unknown or mismatched structures fall back to a single
literal paragraph containing the original source. Nesting is capped before
building an excessively deep document.

Each call parses one complete source snapshot. Append-only consumers can cache
the returned value and parse again only when source changes. Changed-suffix
reuse is intentionally deferred until measurement proves full-snapshot parsing
is material.

## Rendering

The document root and nested block containers are full width and shrinkable.
Paragraphs and headings flatten inline semantics into GPUI `StyledText` runs.
Inline code changes font family and background; links are colored and
underlined but not interactive.

List rows use a fixed marker column and a `min_w_0` vertical item body. Block
quotes use an accent edge and the same shrinkable vertical body. Tables use
equal-width shrinkable columns and wrap cell content. Fenced code is plain
monospace text in an elevated surface; language labels are retained, while
syntax highlighting is a later enhancement.

All colors and measurements resolve from chrome tokens and type roles. The
renderer creates no scroll region; the consuming surface owns scrolling and
follow behavior.

## Dependency and boundaries

`pulldown-cmark` is the only parsing dependency. The module has no product ids,
protocol types, persistence, host bridge, commands, or application messages.
Other GPUI Component controls can remain in consuming applications; replacing
Markdown does not imply replacing the rest of that component stack.
