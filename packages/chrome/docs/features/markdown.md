# Native Markdown

## Overview

The chrome kit turns Markdown source into theme-aware GPUI document elements.
It is suitable for committed documents and repeatedly updated streaming
snapshots without requiring an HTML or web-view rendering path.

Apps integrate through `piko_chrome::components::markdown` and treat
`MarkdownDocument` as an opaque cached render input.

## Behavior

- Plain text, paragraphs, headings, emphasis, strong text, strikethrough,
  inline code, links, lists, task markers, block quotes, fenced code, tables,
  hard breaks, and thematic breaks have semantic presentation.
- Ordered and unordered list items retain nested blocks and continuation
  paragraphs in a vertical item body.
- Any source remains visible. Unsupported or structurally invalid input falls
  back to literal text rather than producing an empty document.
- Raw HTML is displayed literally and is never interpreted as application UI.
- Images display alternative text without loading remote content.
- Links remain inert presentation data; consuming applications do not receive
  implicit navigation or network side effects.
- The active chrome palette and density roles determine all presentation.

## App responsibilities

- Keep original source authoritative and treat parsed documents as
  presentation state.
- Parse accumulated streaming text rather than individual transport chunks.
- Cache parsed documents when unrelated repaints should not repeat parsing.
- Decide which product content is Markdown; the kit does not infer message
  roles or content types.
- Supply any future external-link confirmation or navigation behavior outside
  the renderer.

## Non-goals

- A Markdown editor or source/preview control.
- HTML, script, remote image, widget, math, or diagram execution.
- Product message ids, streaming protocols, storage, or scroll-follow policy.
- A patch-based incremental CommonMark parser.
- Rich-text selection or export.
