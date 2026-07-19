# GUI Chrome — Tool-Window Row Layout

> Status: implemented
> Visual rules: [UI Guidelines](../ui-guidelines.md) §IslandPanel / §Trees
> Related: [GUI Chrome Presentation](../features/chrome-presentation.md) (icons / type / copy only)
> Parent: [GPUI Desktop Client Design](overview.md)

## 1. Purpose

Define a single horizontal geometry for tool-window island headers and tree
rows so trailing actions and disclosure gutters align without per-island
padding math.

This is chrome layout ownership. It is independent of Sessions (or any other
island) product behavior. Islands only fill slots; they never invent right-edge
insets to “line up with the header.”

## 2. Problem

Before this contract, two paths owned similar rows independently:

| Path | Module | Right edge |
|---|---|---|
| Island header | `chrome/island/panel.rs` | Nested insets + free trailing actions; no disclosure gutter |
| Tree row | `chrome/widgets/tree_list.rs` | Intrinsic trailing content, then fixed disclosure gutter |

Any island that places an action in both places (header and row) must hand-tune
alignment. That leaks layout policy into product panels.

## 3. Responsibility split

| Owner | Owns | Does not own |
|---|---|---|
| `theme/metrics.rs` | Inset and rail widths (single numeric source) | What the action does |
| `chrome/island` | Header rendered through the shared row geometry | Domain labels / session facts |
| `chrome/widgets/tree_list` | Tree rows through the same geometry | Which rows are expandable |
| Islands (`islands/*`) | Slot content: title, label, leading, detail, accessory content | Horizontal edge padding for alignment |

Presentation marks (icon glyph, type role, `t!` copy) remain under Chrome
Presentation. This document only constrains **slot geometry**.

## 4. Shared geometry: tool-window row

One row model for headers and flattened tree rows:

```text
[ inset | leading? | main (flex_1) | detail? | disclosure | accessory | inset ]
```

| Slot | Filled by | Width policy |
|---|---|---|
| `inset` | chrome | Fixed metrics value; identical for header and tree row |
| `leading` | island (optional) | Fixed 16 px (existing `row_leading`) |
| `main` | title (header) or primary label (row) | `flex_1`, truncate |
| `detail` | island (optional, read-only) | Intrinsic; long context such as role · activity |
| `disclosure` | chrome | Fixed 16 px; empty gutter when the row is not expandable |
| `accessory` | island (optional content) | Fixed 24 px rail, always reserved; contains one centered Meta or Action at the right edge |

Header and tree rows always reserve both trailing rails. Accessory content is
mutually exclusive: a row may contain read-only Meta or an interactive Action,
never two horizontal positions selected by content semantics. Empty accessory
and disclosure rails remain as spacers. This keeps header actions, row actions,
and compact metadata on one stable right-edge center line. Disclosure sits to
its left because it describes tree structure; accessory remains the terminal
row control or summary.

For Sessions, Open Directory, per-directory New Session, and every Session's
message count use the accessory rail. Counts include zero. Agents use `detail`
for role and activity text. Disclosure remains independently owned by chrome.

Depth guides (tree only) sit between inset and leading; they do not change the
right-rail contract.

## 5. API shape (chrome-owned)

Conceptual API — names may land as a `ToolWindowRow` helper or as internal
builders used by `IslandHeader` and `render_tree_row`:

- Header: `title` + optional Action accessory.
- Tree: optional **`detail`** plus one **`accessory`** enum (`Meta` or `Action`).
- Disclosure: islands declare `has_children` / `expanded` only; chrome draws
  the chevron or empty gutter.

Islands must not:

- add outer horizontal padding to “match” the header;
- place expandable chevrons themselves;
- put compact metadata in `detail` merely to avoid reserving the accessory;
- put interactive controls in a Meta accessory or read-only text in an Action
  accessory.

## 6. Metrics

Add (or rename to) explicit tool-window constants on `UiMetrics`, for example:

- `tool_row_inset`
- `tool_accessory_width`
- `tool_disclosure_width` (16 px, already implied by Trees guidelines)

The header and scroll viewport share the same outer content gutter; their rows
then share `tool_row_inset`. Call sites keep using shared compact ghost icon
buttons; the rail, not the button, owns alignment.

## 7. Validation

- Unit or layout tests: under the same metrics, header and tree accessory rails
  share the same center line and disclosure left edges align.
- Sessions render an accessory for every message count, including zero.
- Agents / Tree / Sessions all continue to call `render_tree_list` / shared
  header; no island-local alignment flags.

## 8. Non-goals

- Changing Sessions / Agents / Tree product actions (those belong in Workbench
  feature docs).
- Timeline / Composer reading-column insets (different chrome contract).
- Virtualizing tree lists or redesigning disclosure hit-testing beyond the
  existing Trees rules.
- Chrome Presentation icons, typography roles, or locale catalogs.
