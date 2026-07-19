# Piko GUI UI Guidelines

Status: normative for the macOS GPUI client.

These guidelines translate the useful parts of JetBrains Fleet's Islands UI
into piko's conversation Workbench. They define a product-specific system; they
do not require pixel-for-pixel imitation of an IDE or custom replacements for
working GPUI Component controls.

Chrome icons, named type roles, and the English chrome string catalog are
contracted in
[GUI Chrome Presentation](../../../docs/gui-chrome-presentation-feature.md)
and designed in
[GUI Chrome Presentation Design](../../../docs/gui-chrome-presentation-design.md).

### Icons

- Use `PikoIcon` / `theme::icon` for chrome actions, Empty / Loading marks, and
  disclosures. Do not introduce new Unicode glyph icons.
- Empty / Loading marks: Sessions/no-session `Circle`, Timeline
  `MessageSquare`, Agents `Bot`, Tree `Network`, Loading `CircleDashed`, Error
  `TriangleAlert`.
- Sessions rows: directory `Folder` / `FolderOpen`, session `MessageSquare`.
- Timeline roles: user `User`, assistant `Bot`; tools use `Wrench` on chips.
  Thinking is muted text (no Brain row chrome in Timeline).
- Agents rows: `Bot`.
- Composer: target `Bot`; model `Cpu`; thinking `Brain`; Send `Send`; Stop
  `CircleStop`. Activity header `Activity` (status via icon/summary color);
  activity items use kind icons (`Bell` / `Wrench` / `TriangleAlert` / …).
- Tree rows by kind: user `User`, assistant `Bot`, tool `Wrench` (including
  message-shaped tool call/result entries), model `Cpu`, thinking `Brain`,
  branch `GitBranch`, compaction `Layers`, other `Circle`.
- Keep 6 px status / role / connection markers as colored dots — they are not
  icons. Timeline still uses a left accent border alongside role icons.

### Typography

- Prefer `theme::text(TextRole::…)` / `label_text` / `body_markdown` over
  one-off size pairs. Numbers live in `metrics.rs`; application lives in
  `typography.rs`.
- Timeline conversation body: plain text → `TextRole::Body`; markdown →
  `body_markdown` (Body 14/21, heading base = body, ~12 px paragraph gap,
  dark highlight theme). Place markdown in a `w_full` container so list item
  text is not clipped. Do not call `text_size` with metrics outside
  `theme/typography.rs` and `theme/icons.rs`.

### Chrome copy

- GUI-owned chrome strings go through `crate::t!("…")` and
  `packages/gui/locales/gui.yml` (`en` only for now).

## 1. Design goals

1. Keep the active conversation visually dominant.
2. Separate navigation, work, and inspection without heavy borders.
3. Use compact information density suitable for a professional tool.
4. Reveal strong color only for focus, selection, status, and primary actions.
5. Preserve native macOS window behavior and accessible component defaults.

External evidence behind the system:

- [Fleet's New Islands UI](https://blog.jetbrains.com/fleet/2024/12/introducing-fleets-new-islands-ui/)
  describes task panels as distinct islands placed on one scalable canvas,
  transparent component styling, a unified type scale, and a balanced,
  accessible palette.
- [Fleet theme key changes](https://www.jetbrains.com/help/fleet/changes-in-json-keys-for-theme-plugins.html)
  distinguish `background.primary` (window canvas and header) from
  `island.background` (editor and tool panels), and require consistent
  alpha-based list-item states.
- [JetBrains Compact Mode](https://blog.jetbrains.com/idea/2023/03/new-ui-enhancements-in-intellij-idea-2023-1/)
  reduces toolbar/header heights, spacing, padding, icons, and buttons as one
  coordinated density change.

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

- The title bar and StatusBar belong to the window canvas. They are never
  islands and never receive floating margins or rounded corners.
- Layout units are islands only: Sessions, Timeline, Composer, Agents, and
  Tree. The right column is just Agents stacked above Tree when
  either is open.
- Timeline fills the remaining center-column height and owns its vertical
  scroll. Composer keeps its intrinsic height at the bottom, independent of
  Timeline content length.
- Activity belongs to the Composer island as its operational status layer.
- Agent Tree and Tree are independent islands separated by the
  same 8 px canvas gutter used horizontally. Their resize handle is invisible
  until dragging.

## 3. Surface hierarchy

| Level | Token | Fleet Dark value | Use |
|---|---|---:|---|
| Canvas | `canvas` / `chrome` | `#090909` | title bar, gutters, StatusBar |
| Island | `surface` | `#18191B` | Sessions, Timeline, Composer, Agents, Tree |
| Elevated | `elevated` | `#252629` | Composer, tool detail, hover/selection |
| Separator | `border` | `#3E4147` | internal dividers and focus-neutral edges |
| Focus | `ring` | `#2A7DEB` | keyboard focus and active input only |

Rules:

- Islands use an 8 px outer gutter, 8 px between adjacent islands, and a 10 px
  radius. Canvas contrast defines their outer edge; islands have no outline.
- Strong `border` color is reserved for controls and structural dividers that
  communicate an actual split or resize relationship.
- Do not wrap every row or message in a bordered card.
- Selected list/tree rows use an elevated fill. A blue rectangular border is
  not a generic selection treatment.

## 4. Density and typography

Implementation ownership under `src/theme/`:

| Concern | Module |
|---|---|
| Spacing, type-scale numbers, layout constants | `metrics.rs` |
| Semantic colors | `tokens.rs` |
| Applying type roles to UI | `typography.rs` (`TextRole`, `body_markdown`) |
| Icon box sizes | `icons.rs` (`IconSize`) |

Call sites use `theme::text` / `label_text` / `body_markdown` / `row_leading`.
They must not read `metrics().*_size` to set fonts directly.

| Role | Size / line height |
|---|---:|
| Metadata | 12 / 16 px |
| Control or tree label | 13 / 18 px |
| Conversation body | 14 / 21 px |
| Panel header | 40 px high |
| Native-integrated title bar | 34 px high |
| Compact status bar | 28 px high |
| Tree/list row | 32 px high |

- Use the 4 / 8 / 12 / 16 px spacing scale.
- Use the system UI font for interface and mixed CJK text. Use monospace only
  for code, logs, ids, and tool detail.
- Limit Timeline conversation reading width to 880 px and center it in the
  island. Activity and Composer fill the center column width and track the
  window; they are not capped at the reading width.
- Preserve text hierarchy with weight and muted color before increasing size.
- Truncate Sessions / Agents / Tree list labels to one line. Conversation
  prose may wrap.

## 5. Component rules

### IslandPanel

Workbench islands use the shared `IslandPanel` chrome in `src/chrome/island/`:

- Shell: surface fill, 10 px radius, no idle outline.
- Header: optional. Tool windows (Sessions, Agents, Tree, Sheet) set
  `IslandHeader::Title`. Timeline and Composer omit the header
  (`.header(...)` not called). Composer uses `.scroll(false)` + `.fill(false)`
  for intrinsic height; Timeline uses the shared scroll viewport with an
  injected `ScrollHandle`.
- Content states: `Ready` (default via `IslandPanel::new`), `Loading`,
  `Empty`, or `Custom` full override. Loading/Empty use a shared centered
  placeholder (`IslandPlaceholder`: optional icon or media element, title,
  optional subtitle, optional action). Islands override by changing those
  fields or by using `IslandBody::Custom` / `IslandPanel::custom`.
- Content scrolling (Ready only, default on): themed vertical scrollbar via
  IslandPanel's viewport (`ScrollableElement` + theme `scrollbar` /
  `scrollbar_thumb`). A thumb paints only when content overflows the viewport
  vertically (gpui-component overflow gate). Timeline injects its own
  `ScrollHandle` for follow/scroll-to-bottom; Sessions / Agents / Tree use a
  keyed handle. The island shell clips to its 10 px radius.
  Loading/Empty/Custom do not use the list scrollbar.
  Horizontal overflow for Tree is deferred (labels truncate for now).
- `.fill(false)` keeps intrinsic height (Composer). Default `.fill(true)`
  fills the parent slot.
- All scrolling islands (Sessions, Timeline, Agents, Tree) share this viewport.

### Title bar

- Use GPUI's native-integrated transparent title bar so macOS traffic lights,
  dragging, and double-click behavior remain native.
- Keep an 80 px traffic-light safe area on both sides of centered title text.
  The right inset compensates for GPUI's native left inset so the context is
  optically centered in the whole window rather than the remaining content.
- Vertically center 13 / 18 px title text with the macOS traffic lights in the
  standard 34 px native-integrated bar.
- Center the compact `piko / session` context.
- Keep actions sparse; use icon/ghost controls only when the action exists.

### Session navigation

- Built with `IslandPanel` (title + `+` action).
- Rows use the shared list-item states and show metadata with muted text.

### Timeline

- Timeline is a first-class island and the only vertically scrolling region in
  the center column. Its flex container must be shrinkable so long content
  never changes Composer position or height.
- Render messages as a continuous reading document, not a stack of chat
  bubbles or full-width ruled rows.
- Use 12 px vertical rhythm and a 6 px role marker. Do not draw a persistent
  role rail or horizontal separator across the reading column.
- User prompts use an elevated rounded block. Assistant answers remain open on
  the island surface so longer prose reads like a document.
- System messages stay on the island surface with muted text and reduced
  emphasis. Thinking is subordinate to the committed answer: muted text with a
  light left border, always expanded. Do not add a redundant `thinking`
  heading, prefix, or right-side Detail control.
- Tool calls are left-aligned compact chips (`Wrench` + name · status). Click
  the chip to expand args/result downward and grow the reading column (no
  Popover, no right-aligned Detail button). Status uses `info` while running,
  `success` when completed, and `danger` when failed.
- Render timeline rows in hostd / Client Core order. Do not bucket or reorder
  thinking, tools, and body across rows. Thinking on a message keeps muted
  text with a light left border; tool chips stay left-aligned compact chips.
- Role color identifies authorship; status color identifies runtime state. Do
  not use warning or danger merely as decoration.

### Activity

- Collapsed Activity is a quiet 32 px status row above Composer, not a
  persistent card. It has no border or background until hover.
- Status uses the `Activity` icon tint and summary color: muted for idle,
  `info` for running, `warning` for an action the user must take. Do not draw
  a separate status dot beside the icon.
- Header content uses `space_sm` horizontal padding inside the hit target.
  The whole row toggles expand/collapse. A muted disclosure chevron sits on
  the trailing edge for visual balance (right when collapsed, down when
  expanded); it is not a separate hit target.
- Expanded Activity uses one elevated rounded container. Items remain flat
  rows with kind icons; do not put a card around every event.
- Keep operational detail here instead of competing with conversation prose in
  Timeline.

### Composer

- Treat Composer as the only persistent interaction focus in Conversation. It
  is a first-class island with intrinsic height and no outer border.
- Place the input on the elevated surface inside the Composer island so its
  editable area remains immediately legible. The component focus ring is the
  only strong outline.
- Default the multi-line input to three rows and auto-grow up to twelve before
  scrolling internally.
- Activity and Composer share one full-width column inside the island so their
  edges stay aligned as the window resizes. Inset that column with the same
  horizontal island padding as Timeline (`space_lg`); do not indent the
  Composer island shell relative to Timeline in the center column.
- Keep the input visually primary and actions in one compact footer with 8 px
  internal padding.
- Target uses the accent color. Model and thinking controls use muted ghost
  styling; Stop uses danger text without a filled background.
- The Send action is the only persistent filled primary action.
- Keep an 8 px canvas gutter between Timeline and Composer.

### Trees

- Use one 32 px row per node with 16 px depth slots and a fixed **trailing**
  disclosure column. Indentation and expand/collapse apply only at branch
  points (a visible parent with two or more filtered children). Single-child
  chains stay flat, always visible, and show no chevron. Leaves keep the empty
  trailing disclosure gutter so labels stay aligned. Hide bookkeeping entries
  (`model_change`, `thinking_level_change`, session info, labels, …) — same
  default as TUI. `parentId` remains the path edge — branch storage is
  one-parent-many-children, not a special flag.
- Draw subtle vertical depth guides to make ancestry scannable.
- Give disclosure arrows their own hit target on the trailing edge and do not
  activate the row when toggling expansion.
- Keep the primary label on one line. Active path is color + weight (fg /
  semibold vs muted off-path); leaf uses accent, preview uses warning. Put
  compact kind/activity metadata beside the label when needed, not a "path"
  badge.
- Hover and selection use background fills; focus may additionally use the
  theme focus ring.

### StatusBar

- Always edge-to-edge, 28 px high, single-line, and read-only.
- Use 8 px horizontal padding so its content aligns with the island gutter.
- Vertically center the 6 px connection marker and 12 / 16 px metadata text.
- Apply the shared 1 px upward optical correction to the complete content row;
  never offset the dot and label independently.
- Do not add an outer margin, radius, top separator, or full border. Canvas
  color and placement define the bar.
- Left side owns connection; right side owns cumulative usage/context when
  available.

## 6. Responsive behavior

- TitleBar left/right panel icons toggle the Sessions column and the Agents+Tree
  column. Selected state follows effective dock visibility; `cmd-b` / `cmd-i`
  share those actions.
- Dock when prefs and width allow (center minimum 620 plus column minima and
  gutters). If both columns are preferred but width is tight, collapse the
  right column first, then the left. Auto-collapse does not clear open prefs.
- Native window minimum matches that center budget: width =
  `CENTER_MIN_WIDTH + 2×gutter` (636), height 600, so the window cannot shrink
  below a usable single-column Workbench.
- When a preferred column cannot dock, the toggle opens a Sheet instead (left
  Sessions; right Agents above Tree in the same vertical stack).
- Removing an island also removes its gutter slot; do not leave empty rails.
- Persist user widths, but always protect the center-column minimum width.

## 7. Review checklist

- [ ] Canvas is visible between all five first-class islands.
- [ ] Title bar and StatusBar are flush with window edges.
- [ ] No outline is drawn around islands, TitleBar, or StatusBar.
- [ ] No duplicate Session header appears above Timeline.
- [ ] Timeline reads as one document without full-width message rules or role
      rails.
- [ ] Long Timeline content scrolls internally and never moves Composer.
- [ ] User and tool entries are elevated while assistant prose stays open.
- [ ] Collapsed Activity is a quiet row; only expanded Activity forms a
      container.
- [ ] Only one filled primary action competes for attention in Composer.
- [ ] Tree depth is readable without blue outline cards.
- [ ] CJK and Latin baselines remain aligned at 100% and Retina scaling.
- [ ] Wide lines stop at the reading-width limit.
- [ ] Focus remains visible independently of hover and selection.
