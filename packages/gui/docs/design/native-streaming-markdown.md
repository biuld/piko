# Native Streaming Markdown Design

Status: Implemented

Feature contract: [Native Streaming Markdown](../features/native-streaming-markdown.md)

## 1. Purpose

Replace the Timeline's GPUI Component Markdown path with a piko-owned,
semantic GPUI renderer. The renderer parses accumulated assistant text with
`pulldown-cmark`, converts parser events into an owned document model, and maps
that model to ordinary GPUI elements.

The first release deliberately favors a small, deterministic pipeline over a
custom incremental parser. Existing host polling already coalesces updates at
50 ms, so each changed assistant body may be parsed as a complete snapshot.
Parsed documents are cached by message identity and source text so unrelated
window repaints do not parse them again.

## 2. Decisions

1. `piko-chrome` owns the generic Markdown document model, parser adapter,
   style contract, and GPUI renderer.
2. `piko-gui` owns the product decision to render assistant Timeline content as
   Markdown and supplies stable message identity plus streaming state.
3. `pulldown-cmark` is the parser. The GUI uses the same Markdown family and
   extensions as the TUI, but does not share the TUI's flattened Ratatui output.
4. Streaming parses the accumulated source snapshot, never an individual model
   delta.
5. The first release reparses a changed document in full. It does not maintain
   parser continuation state or patch an AST in place.
6. Parser output first becomes an owned semantic model. GPUI rendering does not
   consume the parser event iterator directly.
7. Original transcript text remains authoritative. Parsed and tentative state
   is presentation-only and does not change protocol or host storage.

## 3. Current state

Committed assistant rows call GPUI Component `TextView::markdown` through the
chrome typography helper. Realtime assistant rows use plain body text and
switch to Markdown only after commitment. This creates two problems:

- list layout and continuation behavior inherit upstream `TextView` defects;
- a response changes presentation at the commit boundary.

Client Core already assembles ordered realtime deltas into one draft string,
and the GUI bridge polls at 50 ms. That accumulated draft is the parser input;
transport chunks are not Markdown syntax boundaries.

## 4. Responsibility boundaries

| Layer | Responsibility |
|---|---|
| hostd / protocol | Preserve and deliver authoritative message text and lifecycle |
| client-core | Order, deduplicate, and accumulate realtime text deltas |
| GUI Timeline VM | Mark assistant prose as Markdown and expose streaming state |
| GUI Timeline Markdown cache | Cache parsed documents by message id and synchronize them when source changes |
| chrome Markdown parser | Convert source text into a domain-free semantic document |
| chrome Markdown renderer | Convert semantic blocks and inlines into themed GPUI elements |
| chrome theme | Supply Markdown typography, spacing, surfaces, and semantic colors |

No protocol DTO, host command, persistence schema, setting, overlay, focus
contract, or island message changes. Timeline keeps this cache in a dedicated
`markdown_cache` module rather than coupling rendering back to the island view.

## 5. Chrome module boundary

The reusable API lives at `piko_chrome::components::markdown`, matching its
source ownership directly:

```text
components/markdown/
  mod.rs          public opaque document and parse/render entry points
  model.rs        private owned block and inline model
  parse/
    mod.rs        pulldown-cmark adapter and fail-soft entry point
    builder.rs    event-stack builder
    frame.rs      parser stack frames
  render/
    mod.rs        document layout and block dispatch
    inline.rs     styled inline text construction
    blocks.rs     paragraph, heading, quote, list, rule, and code layout
    table.rs      table measurement and row layout
```

`piko-chrome` adds `pulldown-cmark` as an allowed dependency. The module remains
independent of message roles, session ids, realtime protocol events, and piko
Timeline types.

The existing `body_markdown` helper and `TextViewStyle` are removed after the
Timeline migration. `TextRole::Body` and the underlying typography metrics
remain the basis of prose rendering. This change removes only Markdown use of
GPUI Component; Input, menus, overlays, and other component usage remain.

## 6. Semantic document model

The parser adapter builds a small owned tree rather than exposing
`pulldown-cmark` types across the renderer boundary.

```rust
pub struct MarkdownDocument { /* private semantic tree */ }

struct MarkdownNode<T> {
    pub source_range: Range<usize>,
    pub value: T,
}

enum MarkdownBlock {
    Paragraph(Vec<MarkdownInline>),
    Heading { level: u8, content: Vec<MarkdownInline> },
    BlockQuote(Vec<MarkdownNode<MarkdownBlock>>),
    List(MarkdownList),
    CodeBlock { language: Option<String>, code: String },
    Table(MarkdownTable),
    ThematicBreak,
}

enum MarkdownInline {
    Text(String),
    Styled { kind: InlineStyle, children: Vec<MarkdownInline> },
    Code(String),
    Link { destination: String, children: Vec<MarkdownInline> },
    ImageAlt(Vec<MarkdownInline>),
    SoftBreak,
    HardBreak,
}
```

Only the document handle and parse/render functions are public. This prevents
application code from depending on parser-specific tree details while keeping
the model fully inspectable in crate-local parser tests.

Lists retain ordered start values, nesting, item boundaries, and all child
blocks within each item. Continuation paragraphs are vertical children of the
same item; they are never placed in the marker row's horizontal text container.
This makes the upstream continuation-paragraph defect structurally impossible.

Source ranges come from `pulldown-cmark`'s offset iterator. They support parser
tests, stable diagnostics, and a future changed-suffix optimization; they are
not exposed as transcript authority.

## 7. Parser behavior

The parser enables tables, strikethrough, and task-list markers. It follows
these rules:

- any input produces a document or literal fallback; malformed Markdown is not
  a fatal UI error;
- raw HTML events become literal text and never become GPUI elements;
- images retain and render alternative text but do not initiate network work;
- unknown code languages retain plain monospace code;
- soft breaks become spaces in prose, while hard breaks create explicit lines;
- unsupported events preserve visible textual content where possible.

The event adapter uses explicit block and inline stacks. End events must match
the current container; unexpected combinations return a parse error that the
entry point converts to one plain-text paragraph. This keeps the renderer total even
if parser extensions change.

The parser does not append synthetic closing delimiters in the first release.
`pulldown-cmark` already treats any source as a document, including an
unterminated fenced block. Synthetic repair can be evaluated later only if
visual tests demonstrate a meaningful streaming problem.

## 8. GPUI rendering

| Markdown semantic | GPUI structure |
|---|---|
| document | full-width vertical flex container |
| paragraph | wrapping `StyledText` using Body 14/21 |
| heading | wrapping `StyledText` with level-based size and semibold weight |
| inline emphasis/strong/strike/link/code | highlight ranges within one `StyledText` |
| block quote | horizontal accent edge plus a minimum-width-zero vertical body |
| unordered list | marker column plus minimum-width-zero vertical item body |
| ordered list | measured marker column plus minimum-width-zero vertical item body |
| fenced code | elevated full-width monospace block with optional language label |
| table | full-width row/column layout with wrapping cells and restrained separators |
| thematic break | one semantic separator using the border token |

Every wrapping flex child uses full available width where appropriate and
`min_w_0` at nested horizontal boundaries. List markers occupy their own fixed
column; item bodies are vertical flex containers. Nested lists recurse within
the item body.

The first release does not make links interactive. Link destination ranges are
retained in the semantic model so a later safe-navigation feature can wrap
them in `InteractiveText` without changing parsing.

Selectable document interaction is a separate extension described by
[Timeline Text Selection](timeline-text-selection.md). It keeps this semantic
parser and block layout, replacing only leaf text elements with selectable
wrappers and adding a stateful document view for Timeline consumers.

The first release renders fenced code as plain monospace text and retains an
optional language label. Syntax highlighting is a later renderer enhancement;
it must be keyed by language and code hash and must never be required for
document layout correctness.

## 9. Streaming lifecycle and caching

`TimelineIsland` keeps a presentation cache keyed by Timeline row id. Each
entry contains the last source text and parsed document. Streaming state stays
on the Timeline row because it affects decoration, not Markdown semantics.

On `apply_timeline`:

1. Remove cache entries for rows no longer present.
2. Leave an entry unchanged when its source and rendering mode are unchanged.
3. Parse the complete accumulated source when an assistant source changes.
4. Replace the cached document atomically.
5. Notify GPUI once for the refreshed Timeline projection.

Realtime and committed versions of the same message id use the same cache
entry. Because each snapshot is already parsed as a complete document,
commitment reuses the parsed document when the source is identical and changes
only the streaming decoration. The Timeline dirty fingerprint must still
include row kind, streaming state, Markdown mode, and body content rather than
only the last body's length and id.

The existing 50 ms bridge poll is the initial coalescing boundary. No second
Markdown timer is added until measurement shows it is needed. Parsing begins on
the foreground thread in the first release because documents are expected to
be small and this avoids cross-thread entity complexity. A benchmark gate moves
parsing or code highlighting to the background if a refresh exceeds the frame
budget.

No block diff is required initially. If profiling later shows layout churn,
the cache can compare source-ranged top-level blocks and retain the longest
identical prefix. This is an optimization and must not change output semantics.

## 10. Scroll and layout stability

Markdown remains inside the Timeline's existing scroll viewport. It does not
create nested vertical scrolling. Streaming growth uses the existing follow
policy:

- when attached near the end, growth continues following the message;
- when detached, parsing and changing block heights do not move the reader;
- commitment does not replace the row or reset its scroll identity.

Tables may wrap cells in the first release rather than introduce a nested
horizontal scrollbar. Extremely wide unbroken text uses the same clipping and
wrapping policy as code blocks and must not widen the island.

## 11. Safety and resource limits

- Raw HTML is never interpreted.
- Parsing never opens links, reads files, downloads images, or executes code.
- URL destinations remain inert data in the first release.
- Parser and renderer recursion must have a bounded nesting policy; content
  beyond the bound falls back to literal text within the affected subtree.
- A future syntax highlighter must have a large-message threshold and disable
  highlighting before basic Markdown structure.
- Parse failure, unsupported syntax, and highlight failure degrade locally and
  never suppress source text.

## 12. Validation

### Parser tests

- paragraphs, headings, soft and hard breaks;
- nested emphasis, strong, strikethrough, inline code, and links;
- ordered and unordered nested lists;
- list continuation paragraphs and loose lists;
- block quotes containing multiple blocks;
- fenced and unterminated code blocks with known and unknown languages;
- tables, thematic breaks, raw HTML, images, empty input, and mixed CJK;
- every fixture preserves all user-visible text.

### Renderer and theme tests

- semantic nodes map to the expected GPUI container shape;
- all nested horizontal content uses a shrinkable body;
- light and dark palettes resolve without hard-coded colors;
- headings keep the compact conversation scale;
- unknown code languages use the plain fallback;
- raw HTML and image sources cannot trigger external behavior.

### GUI integration tests

- realtime assistant rows select Markdown rendering;
- draft-to-commit replacement keeps one row even when length is unchanged;
- source changes invalidate only the matching cache entry;
- detached Timeline follow remains detached as block heights change;
- committed and streaming snapshots with the same source produce the same
  semantic document.

Focused validation runs formatting, chrome tests and clippy, then GUI tests and
clippy. Workspace validation is not required because protocol and host crates
do not change.

## 13. Implementation mapping

1. Chrome owns the semantic model, parser adapter, fixtures, and dependency.
2. Chrome owns themed inline, block, list, code, and table renderers.
3. Timeline owns the parsed-document cache and enables Markdown for realtime
   assistant rows.
4. The native renderer replaces `body_markdown` and Timeline's GPUI Component
   `TextView` Markdown path.
5. Parser, streaming projection, both palettes, list wrapping structure, and
   scroll-follow policy are covered by focused tests and existing GUI tests.

These slices intentionally leave the broader GPUI Component dependency in
place. Removing or replacing Input, menus, sheets, notifications, and other
components is a separate architectural decision.
