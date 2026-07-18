# Phase 7 / 8 manual UX checklist

Automated Rust tests cannot verify native GPUI focus, IME, pointer, or window
behavior. Before declaring M4, exercise the following on macOS against a real
hostd process.

## Keyboard main path

- [ ] `cmd-n` create session, `cmd-b` / `cmd-i` toggle Sessions / Agents+Tree
- [ ] `cmd-l` focus Composer, Enter submit, Shift+Enter newline
- [ ] `cmd-.` cancel Turn, `cmd-j` jump to latest when detached
- [ ] Tab / Shift+Tab traverses focusable chrome without trapping in StatusBar
- [ ] Focus ring remains visible on Composer and primary buttons (theme `ring`)

## IME / CJK

- [ ] Composer accepts CJK IME composition (Chinese / Japanese / Korean)
- [ ] Confirming IME candidate inserts text without premature submit
- [ ] Selection + copy/paste of mixed CJK and ASCII works in Composer

## Timeline follow

- [ ] Attached: streaming deltas keep viewport at bottom
- [ ] Detached (scroll up): streaming does **not** move the reader
- [ ] `cmd-j` reattaches follow and scrolls to end (no in-panel Jump button)
- [ ] Opening or hydrating an existing session starts at the top and does not
      implicitly enable bottom-follow
- [ ] Expanding a tool Detail card does not force scroll when detached
- [ ] Switching Agent restores that Agent’s follow preference
- [ ] A long Timeline scrolls inside its own island without moving or shrinking
      Composer
- [ ] Timeline scrollbar is anchored to the viewport edge, spans only Timeline,
      and supports wheel, track, and thumb dragging without moving with content

## Conversation center

- [ ] Assistant prose reads as an open document; user prompts and tool calls
      use lightweight elevated blocks
- [ ] Messages have no full-width separator lines or persistent left role rails
- [ ] Role markers remain distinct without overpowering mixed CJK/Latin body
      text
- [ ] Thinking content is visually subordinate without a visible `thinking`
      heading or prefix
- [ ] Running, completed, and failed tool markers use info, success, and danger
      colors respectively
- [ ] Collapsed Activity is a 32 px borderless row and gains a container only
      when expanded
- [ ] Activity warning color appears only for actionable or failed state; idle
      and queued state remain muted
- [ ] Timeline and Composer are independent islands separated by an 8 px canvas
      gutter
- [ ] Composer is a surface island, while its editable input uses the elevated
      surface with a visible focus ring
- [ ] Target, model, thinking, Stop, and Send form one aligned compact footer;
      Send is the only persistent filled action

## Layout / StatusBar

- [ ] Wide (≥1200): Sessions + center + Agents/Tree; medium hides Agents/Tree; narrow uses Sheets
- [ ] Wide layout shows an 8 px canvas gutter between the three columns
- [ ] Agents and Tree are separate rounded islands with an 8 px vertical gutter
- [ ] Sessions / Agents / Tree scrollbars match Timeline (themed thumb, island
  bottom-right radius preserved; vertical only — Tree labels truncate)
- [ ] TitleBar and StatusBar remain edge-to-edge and are not rounded islands
- [ ] Islands and edge chrome have no decorative outer borders
- [ ] Resizable gutters draw no idle line; drag feedback appears only while resizing
- [ ] Title text is optically centered across the full window, not the area after traffic lights
- [ ] Traffic lights and 13 / 18 px title text share the same vertical center
- [ ] StatusBar uses 8 px horizontal padding and vertically aligns its dot and text
- [ ] Center remains usable with both sidebars closed
- [ ] StatusBar cwd appears only when Session sidebar is hidden; abbreviated
- [ ] Usage shows cumulative tokens/cost when host provides them (no fake limit)

## Visual system

- [ ] Session, Agent, and Tree selections use filled rows, not blue outline cards
- [ ] Tree disclosures toggle without also activating the row
- [ ] Tree disclosures use chevron icons (not Unicode triangles) with a stable
      12 px column
- [ ] Empty / Loading islands show Lucide marks (`Circle` / `MessageSquare` /
      `Bot` / `Network` / `CircleDashed` / `TriangleAlert`) instead of glyphs
- [ ] New Session uses a Plus icon button with an English tooltip
- [ ] Tree depth guides remain aligned while scrolling and resizing
- [ ] Timeline content remains centered at the 880 px reading width; Activity
      and Composer fill the center column and stay edge-aligned with each other
- [ ] Mixed CJK/Latin labels beside icons stay vertically centered at Retina
- [ ] Mixed CJK/Latin labels truncate cleanly without clipping vertically
- [ ] Status / role / connection markers remain 6 px color dots (not icons)

See [`ui-guidelines.md`](ui-guidelines.md) for the normative visual rules.

## Reduced motion

- [ ] With `[gui].reduced_motion = true`, decorative
      spinners / animations are skipped when present

## Notifications

- [ ] Error and disconnect produce a toast; repeated identical errors do not spam
- [ ] Actionable approvals remain in Activity Center / dialogs

## Known limits (documented)

- Protocol `Usage` has no context-window field; StatusBar cannot show `used/limit`
  until host exposes it.
- VoiceOver / platform accessibility announcements are not bridged yet.
- `[gui]` persistence covers pane visibility, split sizes, and reduced motion;
  focus, tree expansion, drafts, and scroll/follow positions remain local.
