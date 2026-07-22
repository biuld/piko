# Piko GUI UI Guidelines

Status: normative for the macOS GPUI client.

Visual and layout rules for the Islands Workbench. Product behavior lives in
[GUI Workbench](features/workbench.md). Chrome icons, named
type roles (`TextRole`), and the English string catalog live in
[GUI Chrome Presentation](features/chrome-presentation.md)
and its [design](design/chrome-presentation.md).

These guidelines define a product-specific system inspired by JetBrains Fleet's
Islands UI (canvas vs island surfaces, compact density). They do not require
pixel-for-pixel imitation of an IDE or custom replacements for working GPUI
Component controls.

## 1. Design goals

1. Keep the active conversation visually dominant.
2. Separate navigation, work, and inspection without heavy borders.
3. Use compact information density suitable for a professional tool.
4. Reveal strong color only for focus, selection, status, and primary actions.
5. Preserve native macOS window behavior and accessible component defaults.

## 2. Window anatomy

```text
native-integrated title bar / window canvas
┌──────────────────────────────────────────────────────────────┐
│  Sessions island  │    Timeline island    │ Agents island   │
│                   │                       ├─────────────────┤
│                   ├───────────────────────┤ Tree island      │
│                   │    Composer island    │                 │
│                   │ Activity / input      │                 │
└──────────────────────────────────────────────────────────────┘
edge-to-edge status bar
```

- Title bar and StatusBar belong to the window canvas. They are never islands
  and never receive floating margins or rounded corners.
- Layout units are the five islands: Sessions, Timeline, Composer, Agents, and
  Tree. The right column stacks Agents above Tree when either is open.
- Timeline fills remaining center-column height and owns vertical scroll.
  Composer keeps intrinsic height at the bottom, independent of Timeline length.
- Activity belongs to the Composer island as its operational status layer.
- Agents and Tree are separate islands with the same 8 px canvas gutter used
  horizontally. Their resize handle is invisible until dragging.
- Center modals (HostPrompt, LocalConfirm, Transient such as Command Palette)
  belong to chrome `OverlayHost`: full-window absolute layer, dimmed backdrop,
  elevated panel. They are not islands and do not use GPUI `open_dialog`.
- Side Sheets (narrow dock fallback) and toast notifications stay on separate
  Root layers; Escape for Sheets is routed through OverlayHost.

## 3. Surfaces

| Level | Token | Fleet Dark value | Use |
|---|---|---:|---|
| Canvas | `canvas` / `chrome` | `#090909` | title bar, gutters, StatusBar |
| Island | `surface` | `#18191B` | Sessions, Timeline, Composer shell, Agents, Tree |
| Elevated | `elevated` | `#252629` | Composer input, tool detail, hover/selection, overlay panels |
| Overlay dim | black ~45% alpha | backdrop behind OverlayHost surfaces |
| Separator | `border` | `#3E4147` | internal dividers and focus-neutral edges |
| Focus | `ring` | `#2A7DEB` | keyboard focus and active input only |

Rules:

- Islands use an 8 px outer gutter, 8 px between adjacent islands, and a 10 px
  radius. Canvas contrast defines the outer edge; islands have no outline.
- Strong `border` is reserved for controls and structural dividers that
  communicate a real split or resize relationship.
- Do not wrap every row or message in a bordered card.
- Selected list/tree rows use an elevated fill. A blue rectangular border is
  not a generic selection treatment.

## 4. Density and typography

Implementation under `src/theme/`:

| Concern | Module |
|---|---|
| Spacing, type-scale numbers, layout constants | `metrics.rs` |
| Semantic colors | `tokens.rs` |
| Applying type roles | `typography.rs` (`TextRole`, `body_markdown`) |
| Icon box sizes | `icons.rs` (`IconSize`) |

Call sites use `theme::text` / `label_text` / `body_markdown` / `row_leading`.
Do not read `metrics().*_size` to set fonts directly. Do not call `text_size`
with metrics outside `typography.rs` and `icons.rs`.

| Role | Size / line height |
|---|---:|
| Metadata | 12 / 16 px |
| Control or tree label | 13 / 18 px |
| Conversation body | 14 / 21 px |
| Panel header | 40 px high |
| Native-integrated title bar | 34 px high |
| Compact status bar | 28 px high |
| Tree/list row | 32 px high |

- Spacing scale: 4 / 8 / 12 / 16 px.
- System UI font for interface and mixed CJK; monospace only for code, logs,
  ids, and tool detail.
- Timeline reading width caps at 880 px and centers in the island. Activity and
  Composer fill the center column width (not the reading-width cap).
- Prefer weight and muted color over larger sizes for hierarchy.
- Truncate Sessions / Agents / Tree labels to one line. Conversation prose may
  wrap.
- Timeline body: plain text → `TextRole::Body`; markdown → `body_markdown`
  (Body 14/21, heading base = body, ~12 px paragraph gap, dark highlight).
  Place markdown in a `w_full` container so list items are not clipped.

## 5. Chrome marks

Locked Empty / Loading / row icon mappings and chrome copy keys live in
[GUI Chrome Presentation](features/chrome-presentation.md).
Guidelines only fix the visual rules:

- Use `ChromeIcon` / `theme::icon` for chrome actions, Empty / Loading marks, and
  disclosures. Do not introduce Unicode glyph icons.
- TitleBar dock toggles use `PanelLeft` / `PanelLeftFilled` and `PanelRight` /
  `PanelRightFilled` (hollow when closed, filled when docked).
- Streaming assistant progress may use a rotating `Settings` gear; respect
  `[gui].reduced_motion` with a static mark.
- Keep 6 px status / role / connection markers as colored dots — they are not
  icons. Timeline keeps a left accent border alongside role icons.
- GUI-owned chrome strings go through `crate::t!("…")` and
  `packages/gui/locales/gui.yml` (`en` only for now).

## 6. Component rules

### IslandPanel

Shared island chrome in `src/shell/island/`:

- Shell: surface fill, 10 px radius, no idle outline.
- Header: optional. Sessions / Agents / Tree / Sheet use a title header;
  Timeline and Composer omit it.
- Header and tree rows share one tool-window row geometry (inset, main, optional
  detail, fixed accessory rail, disclosure gutter). Islands fill slots only;
  they do not add edge padding to align trailing controls. See
  [Tool-Window Row Layout](design/chrome-tool-window-layout.md).
- Composer: `.scroll(false)` + `.fill(false)` for intrinsic height. Timeline
  uses the shared scroll viewport with an injected `ScrollHandle`.
- Content states: Ready, Loading, Empty, or Custom. Loading/Empty use
  `IslandPlaceholder` (icon, title, optional subtitle/action).
- Ready scrolling: themed vertical scrollbar; thumb only when content overflows.
  Sessions / Agents / Tree use a keyed handle. Horizontal Tree scroll is
  deferred (labels truncate).

### Title bar

- Native-integrated transparent title bar (macOS traffic lights, drag,
  double-click).
- Centered brand mark only: semibold `piko`. Session and project context live
  in Sessions (and abbreviated cwd in StatusBar when Sessions is hidden).
- 80 px safe inset on both sides so the brand is optically centered in the full
  window.
- Vertically center 13 / 18 px title text in the 34 px bar.
- Sparse icon/ghost actions only when the action exists (panel toggles).

### Sessions

- `IslandPanel` with title + Open Directory header action.
- Directory groups expose New Session in the shared accessory rail.
- Every Session shows its message count, including zero, in that same rail.
- Rows use shared list-item states; metadata is muted.

### Timeline

- Only vertically scrolling region in the center column. Long content must not
  move or resize Composer.
- Continuous reading document — not chat bubbles or full-width ruled rows.
- 12 px vertical rhythm; 6 px role marker. No persistent role rail or horizontal
  separators across the reading column.
- User prompts: elevated rounded block. Assistant answers: open on the island
  surface. System: muted, reduced emphasis.
- Thinking: muted text with a light left border, always expanded. No redundant
  `thinking` heading, prefix, or Detail control.
- Tools: left-aligned chips (`Wrench` + name · status). Click expands
  args/result downward (no Popover, no right-aligned Detail). Status: `info`
  running, `success` completed, `danger` failed.
- Preserve hostd / Client Core row order. Role color = authorship; status color =
  runtime state. Do not use warning/danger as decoration.

### Activity

- Quiet 32 px status row above the input inside the Composer island; no border
  or background until hover.
- Status via `Activity` icon tint + summary color: muted idle, `info` running,
  `warning` when the user must act. No separate status dot beside the icon.
- Whole row toggles expand/collapse; muted trailing chevron is visual only.
- Expanded: one elevated rounded container with flat kind-icon rows — no card
  per event.
- Operational detail stays here, not in Timeline prose.

### Composer

- First-class island with intrinsic height and no outer border.
- Island shell is `surface`; input sits on `elevated` inside it. Component focus
  ring is the only strong outline.
- Multi-line input defaults to three rows, auto-grows to twelve, then scrolls.
- Activity and input share one full-width column with the same horizontal
  `space_lg` inset as Timeline.
- Compact footer (8 px padding): target uses accent; model/thinking are muted
  ghost; Stop is danger text without fill; Send is the only persistent filled
  primary.
- 8 px canvas gutter between Timeline and Composer.

### Trees

- 32 px rows, 16 px depth slots, fixed 24 px **accessory** and 16 px
  **disclosure** columns (chrome-owned and always reserved). Accessory content
  is either read-only Meta or interactive Action; optional long **detail** text
  sits before the rails. Disclosure precedes accessory, leaving header actions
  and row accessories on the terminal right-edge rail. Geometry:
  [Tool-Window Row Layout](design/chrome-tool-window-layout.md).
- Expand/collapse only at true branch points (parent with two or more filtered
  children). Single-child chains stay flat with no chevron. Leaves keep the
  empty disclosure gutter to preserve trailing rail alignment.
- Hide bookkeeping entries (`model_change`, `thinking_level_change`, session
  info, labels, …) by default — same as TUI. `parentId` is the path edge.
- Subtle vertical depth guides. Disclosure has its own hit target and must not
  activate the row.
- One-line primary label. Active path: fg + semibold; off-path: muted; leaf:
  accent; preview: warning. Hover/selection = background fills; focus may add
  the theme ring.

### StatusBar

- Edge-to-edge, 28 px, single-line, read-only.
- 8 px horizontal padding aligned with the island gutter.
- Vertically center the 6 px connection marker and 12 / 16 px metadata; apply
  the shared 1 px upward optical correction to the whole content row.
- No outer margin, radius, top separator, or full border.
- Left: connection. Right: cumulative usage/context when the host provides it.

## 7. Responsive behavior

- TitleBar left/right panel icons toggle Sessions and Agents+Tree. Pressed state
  follows effective dock visibility; `cmd-b` / `cmd-i` share those actions.
- Dock when prefs and width allow (center minimum 620 plus column minima and
  gutters). If both columns are preferred but width is tight, collapse right
  first, then left. Auto-collapse does not clear open prefs.
- Native window minimum: `CENTER_MIN_WIDTH + 2×gutter` (636) × 600.
- When a preferred column cannot dock, the toggle opens a Sheet (left Sessions;
  right Agents above Tree).
- Removing an island removes its gutter slot. Persist user widths, but always
  protect the center-column minimum.

## 8. Design review

- [ ] Canvas visible between islands; TitleBar and StatusBar flush, no outlines
- [ ] No duplicate Session header above Timeline
- [ ] Timeline reads as one document; long content never moves Composer
- [ ] User/tool elevated; assistant prose open on the surface
- [ ] Collapsed Activity is a quiet row; only expanded Activity is a container
- [ ] One filled primary action in Composer
- [ ] Tree depth readable without blue outline cards
- [ ] CJK/Latin baselines aligned; reading width respected
- [ ] Focus visible independently of hover and selection
