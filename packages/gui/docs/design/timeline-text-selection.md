# Timeline Text Selection Design

Status: Implemented

Feature contract: [Timeline Text Selection](../features/timeline-text-selection.md)

Related designs:

- [Native Streaming Markdown](native-streaming-markdown.md)
- [Chrome Native Flat Context Menu](../../../chrome/docs/design/context-menu.md)

## 1. Problem

Timeline Markdown currently renders as ordinary GPUI `StyledText` elements.
`StyledText` exposes its `TextLayout`, including byte-index/position mapping,
but does not own drag selection, selection paint, focus, or clipboard behavior.
The current renderer also emits many independent text elements for paragraphs,
markers, nested lists, table cells, and code.

`gpui-component::TextView` has a selectable mode, but adopting it would restore
the same Markdown node renderer whose list continuation layout motivated the
native renderer. Its selection implementation is also coupled to private
TextView nodes and global TextView state, so it is not a reusable adapter for
the piko semantic model.

Selection is therefore added to the native renderer as an independent document
interaction layer. Parsing and layout semantics remain unchanged.

## 2. Decisions

1. `piko-chrome` owns generic selectable-document state, text hit testing,
   selection paint, and clipboard projection.
2. The GUI Timeline owns which rows and fields are selectable and gives each
   selectable row a stable document identity.
3. One Timeline selection group permits one active row selection at a time.
4. Selection ranges are UTF-8 byte ranges over an owned rendered-text
   projection, always normalized to character boundaries.
5. A selection may cross every text fragment in one row, including nested
   Markdown blocks, but cannot cross row identities in the first release.
6. The copied value is semantic visible plain text, not a slice of Markdown
   source and not text reconstructed from screen coordinates.
7. The native flat context menu provides the Copy surface. Selection logic does
   not implement a second popup system.
8. Right-click preserves selection and only opens Copy when the pointer maps
   inside the selected range. Primary-button gestures alone mutate selection.
9. No protocol, hostd, persistence, settings, or transcript DTO changes are
   required.
10. Double-click word expansion follows Unicode Standard Annex #29 through the
    small `unicode-segmentation` crate; it does not use ASCII-only character
    classes.

## 3. Responsibility boundaries

| Layer | Responsibility |
|---|---|
| hostd / client-core | Keep authoritative message and tool text unchanged |
| Timeline VM | Identify prose/code fields and stable row ids |
| Timeline selection coordinator | Create one selection group and synchronize row documents |
| chrome Markdown projection | Produce visible plain text and styled fragments in semantic order |
| chrome selection group | Enforce one active document, focus, range, copy, and clear policy |
| selectable text element | Register `TextLayout`, hit-test positions, and paint range intersections |
| chrome context menu | Anchor and dismiss the conditional Copy action surface |
| system clipboard | Receive one plain-text value |

The selection group and rendered documents are presentation state. The GUI
does not send selection ranges or copied values to hostd.

## 4. Chrome component structure

The reusable interaction code belongs beside other chrome components rather
than inside the Timeline feature:

```text
components/selection/
  mod.rs          public handles and exports
  model.rs        active-row coordination and fragment registry
  region.rs       mouse, focus, Command-C, and Copy menu behavior
  text.rs         StyledText hit testing and selection paint

components/markdown/render/
  ...             block layout emitting selectable fragments
```

The existing parser model remains private and unchanged in authority. A new
projection traversal derives visible text and fragment ranges from that model.
The selection module has no Markdown parser dependency; plain Timeline fields
reuse the same document and element primitives.

## 5. API direction

The intended public surface is stateful because selection must survive GPUI
repaints:

```rust
pub struct SelectionGroup;
pub struct SelectionState;
pub struct SelectableText;

pub fn selectable_region(/* stable id, state, Copy label, child, owner EntityId, App */);
pub fn render_selectable_markdown(
    id: impl Into<SharedString>,
    document: &MarkdownDocument,
    selection: Entity<SelectionState>,
) -> AnyElement;
```

A corresponding plain-document view uses the selection primitives for user,
thinking, system, and expanded tool text without interpreting those strings as
Markdown. Concrete constructors may be refined during implementation, but the
following boundaries are fixed:

- the group is shared by all selectable rows in one Timeline;
- each view owns exactly one row-scoped logical document;
- consumers cannot mutate raw selection offsets;
- the Markdown AST and fragment registry remain private;
- clipboard projection is produced by chrome, not reassembled by the GUI.

The current stateless `render_markdown` entry point may remain for nonselectable
consumers. Timeline migrates to the stateful view.

`unicode-segmentation` supplies word-boundary resolution. The selection module
remains independent of GUI product types and Markdown parsing.

## 6. Rendered-text projection

Each selectable document owns:

```text
plain_text: String
fragments: ordered [document byte range + style/layout identity]
selection: optional anchor/head byte offsets
source_revision: source/projection snapshot used for update policy
```

Projection follows visible semantic order independently of flex structure:

| Semantic content | Plain-text projection |
|---|---|
| inline styled text | displayed characters only |
| soft / hard break | space / newline |
| paragraphs and headings | separated by blank line |
| unordered / ordered item | rendered marker, space, item blocks |
| task item | `[ ]` or `[x]` marker |
| block quote | quoted content without a synthetic `>` marker |
| code block | code only; no fences or language badge |
| table | tab-separated cells, newline-separated rows |
| thematic break | block separator only |
| image | visible alternative text |
| link | visible label only |

Every text element rendered on screen receives a range into `plain_text`.
Separators that are copied but not painted, such as the newline between two
paragraphs, occupy gaps between fragment ranges. A selection crossing the gap
therefore copies the expected structure without requiring an invisible GPUI
text element.

## 7. Layout registry and hit testing

`StyledText::layout()` returns a clonable `TextLayout`. The selectable wrapper
registers, during prepaint, its document range, screen bounds, and layout in a
per-document fragment registry. The registry is rebuilt by generation on each
paint so resized and rewrapped text never uses stale geometry.

Point-to-offset mapping uses these rules:

1. Prefer the fragment whose vertical bounds contain the pointer.
2. Use `TextLayout::index_for_position`; its nearest-index error value is a
   valid clamped local byte offset.
3. Add the fragment's document-range start.
4. Between fragments, choose the preceding end or following start according to
   the nearest vertical edge.
5. Before the first or after the last fragment, clamp to the document bounds.
6. Normalize every result to a UTF-8 character boundary.

This provides one logical selection across independent paragraphs, list marker
columns, nested blocks, table cells, and code elements without changing their
layout containers.

## 8. Selection gestures and focus

```text
primary down on selectable text
  → selection group activates that document and clears another row
  → map pointer to document offset
  → set anchor = head = offset
  → focus the shared selection group

primary drag
  → map current pointer through the latest fragment registry
  → update head and notify the document view

primary up
  → finish dragging; retain normalized non-empty range

double primary click
  → expand around the hit offset to Unicode word boundaries

Command-C
  → copy active document's selected plain-text slice

secondary click inside selection
  → preserve range
  → open native ContextMenu with Copy

secondary click outside selection
  → do not alter the range and do not open Copy
```

Mouse selection stops propagation before row activation handlers run. The
selection group has one focus handle tracked inside the Timeline island, so
Command-C works without removing the island from the shell's logical focus
ring. Closing the context menu restores that same handle and leaves the range
highlighted.

`piko_chrome::components::init(cx)` registers the private Copy action and
Command-C binding under a `PikoTextSelection` key context, alongside the native
context-menu bindings. The GUI calls this component initializer once at startup
after its existing GPUI Component initialization.

A primary click outside selectable text clears the active group selection.
Clicks on controls continue to execute their existing action after the clear.

## 9. Selection paint

Each selectable text element intersects the normalized document range with its
own fragment range and converts that intersection back to local byte offsets.
`TextLayout::position_for_index` and line height produce up to three rectangles:

- first partial line;
- zero or more full middle lines;
- final partial line.

The rectangles are painted over text backgrounds with a translucent selection
color, matching GPUI's selectable TextView behavior while keeping inline-code
backgrounds visible. The text cursor is I-beam over selectable fragments and
remains pointing-hand over interactive controls.

Selection color derives from the chrome ring/accent color with palette-specific
alpha rather than adding a product color:

| Palette | Selection alpha |
|---|---:|
| Dark | 0.30 |
| Light | 0.22 |

Pointer and keyboard copy use the same stored range; paint geometry never
becomes clipboard authority.

## 10. Context-menu integration

The native Context Menu design gains one general rule: a builder that returns
an empty specification does not open a menu. Its request includes the click
position in window coordinates. Timeline supplies one Copy item only when that
position maps inside the active non-empty selection.

The action sequence is:

1. context menu closes;
2. focus returns to the selection group;
3. selected plain text is written once to the clipboard;
4. selection remains active and painted.

The menu component does not know about Markdown, Timeline rows, or clipboard
formatting. It invokes the callback supplied by the selection view.

## 11. Streaming and document updates

When a row's source changes, chrome builds the new plain-text projection before
replacing the old document.

Preserve the selection only when:

- the old selection end is within the new projection; and
- the new projection prefix through that end is byte-for-byte equal to the old
  prefix through that end.

This covers ordinary append-only streaming and syntax changes that alter style
without changing already visible text. If content at or before the selected end
changes, clear the selection rather than silently copying a shifted range.

Removing a Timeline row unregisters its selectable document. If it was active,
the group clears selection and releases its focus ownership safely.

## 12. Timeline integration

Timeline owns one `TextSelectionGroup` Entity. Its presentation cache retains
stateful selectable document views by stable row id alongside the source used
to build them.

| Row content | View mode |
|---|---|
| assistant body | parsed Markdown document |
| user / system / thinking | literal plain-text document |
| fenced code | Markdown code fragment |
| expanded tool args/result | literal or code-like plain-text document |
| labels, status, buttons, chips | existing nonselectable elements |

The cache updates existing entities instead of recreating them on unrelated
Timeline refreshes. This preserves selection, fragment identities, and focus
while still allowing the semantic document to update during streaming.

## 13. Validation

### Pure tests

- semantic projection produces expected paragraph, list, table, code, link,
  and image text;
- byte ranges remain UTF-8 boundaries for mixed CJK and emoji;
- point mapping clamps before, inside, between, and after fragments;
- range/fragment intersection handles forward and backward selections;
- append-only updates preserve selection;
- prefix rewrites and row removal clear selection;
- Copy returns exactly the normalized selected plain text.

### GPUI interaction tests

- drag selection does not activate a row or tool;
- double-click selects one Unicode word;
- selection crosses wrapped lines and Markdown blocks;
- starting in another row clears the first selection;
- Command-C and menu Copy produce identical clipboard text;
- right-click inside opens Copy while outside does not;
- context-menu dismissal preserves the range and restores selection focus;
- resize and rewrap rebuild hit-test geometry.

### Visual review

- dark and light selection colors remain readable over normal and inline-code
  backgrounds;
- nested lists, tables, CJK, emoji, and monospace code select without gaps or
  off-by-one highlights;
- the Copy menu uses the chrome tonal-elevation surface without a shadow;
- scrolling remains smooth with a long Timeline and no active selection.

## 14. Deferred scope

- cross-row and whole-transcript selection;
- drag-edge autoscroll;
- triple-click paragraph selection;
- Shift-click range extension;
- rich clipboard formats;
- interactive link navigation and its precedence with selection;
- accessibility selection announcements beyond GPUI's current role APIs.
