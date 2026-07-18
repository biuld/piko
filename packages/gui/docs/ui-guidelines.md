# Piko GUI UI Guidelines

Status: normative for the macOS GPUI client.

These guidelines translate the useful parts of JetBrains Fleet's Islands UI
into piko's conversation Workbench. They define a product-specific system; they
do not require pixel-for-pixel imitation of an IDE or custom replacements for
working GPUI Component controls.

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
│                   ├───────────────────────┤ Map island      │
│                   │    Composer island    │                 │
│                   │ Activity / input      │                 │
└──────────────────────────────────────────────────────────────┘
edge-to-edge status bar
```

- The title bar and StatusBar belong to the window canvas. They are never
  islands and never receive floating margins or rounded corners.
- Sessions, Timeline, and Composer are first-class islands. The Inspector
  column owns two additional first-class islands: Agents and Conversation Map.
- Timeline fills the remaining center-column height and owns its vertical
  scroll. Composer keeps its intrinsic height at the bottom, independent of
  Timeline content length.
- Activity belongs to the Composer island as its operational status layer.
- Agent Tree and Conversation Map are independent islands separated by the
  same 8 px canvas gutter used horizontally. Their resize handle is invisible
  until dragging.

## 3. Surface hierarchy

| Level | Token | Fleet Dark value | Use |
|---|---|---:|---|
| Canvas | `canvas` / `chrome` | `#090909` | title bar, gutters, StatusBar |
| Island | `surface` | `#18191B` | Sessions, Conversation, Inspector |
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

The implementation source of truth is `src/theme/metrics.rs`.

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
- Limit conversation reading width to 880 px and center it in the island.
- Preserve text hierarchy with weight and muted color before increasing size.
- Truncate navigation/tree labels to one line. Conversation prose may wrap.

## 5. Component rules

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

- The header behaves like a tool-window tab strip, not a marketing header.
- New Session is a compact ghost `+` action with a tooltip.
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
  emphasis. Thinking is subordinate to the committed answer and uses muted
  contrast.
- Tool calls use an elevated rounded block because they are interactive. Their
  own status marker uses `info` while running, `success` when completed, and
  `danger` when failed. Expanded detail drops back to the island surface.
- Role color identifies authorship; status color identifies runtime state. Do
  not use warning or danger merely as decoration.

### Activity

- Collapsed Activity is a quiet 32 px status row above Composer, not a
  persistent card. It has no border or background until hover.
- Use a 6 px state marker: muted for idle/queued, `info` for running,
  `warning` for an action the user must take, and `danger` for failure.
- The disclosure control owns a fixed 12 px column so the summary does not
  shift between states.
- Expanded Activity uses one elevated rounded container. Items remain flat
  rows with compact state markers; do not put a card around every event.
- Keep operational detail here instead of competing with conversation prose in
  Timeline.

### Composer

- Treat Composer as the only persistent interaction focus in Conversation. It
  is a first-class island with intrinsic height and no outer border.
- Place the input on the elevated surface inside the Composer island so its
  editable area remains immediately legible. The component focus ring is the
  only strong outline.
- Keep the input visually primary and actions in one compact footer with 8 px
  internal padding.
- Target uses the accent color. Model and thinking controls use muted ghost
  styling; Stop uses danger text without a filled background.
- The Send action is the only persistent filled primary action.
- Keep an 8 px canvas gutter between Timeline and Composer.

### Trees

- Use one 32 px row per node with a fixed disclosure column and 16 px depth
  slots.
- Draw subtle vertical depth guides to make ancestry scannable.
- Give disclosure arrows their own hit target and do not activate the row when
  toggling expansion.
- Keep the primary label on one line. Put compact kind/path/activity metadata
  beside it, not in a bordered sub-card.
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

- Wide: show all three islands.
- Medium: keep Sessions and Conversation; expose Inspector through its action.
- Narrow: keep Conversation and open Sessions/Inspector in Sheets.
- Removing an island also removes its gutter slot; do not leave empty rails.
- Persist user widths, but always protect the Conversation minimum width.

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
