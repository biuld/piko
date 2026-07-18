# GUI Chrome Presentation

> Status: implemented feature contract (icons + typography + en chrome catalog)
> Related: [GUI Workbench](gui-workbench-feature.md),
> [UI Guidelines](../packages/gui/docs/ui-guidelines.md)
> Decisions locked: Empty icons (§Icons), Activity templates fully keyed,
> v1 locale = `en` only

## Overview

Chrome Presentation is the Workbench's shared visual language for chrome that
is not conversation content: icons, type roles, and a keyed chrome copy catalog.

Today the Workbench already has surfaces, density, and status color markers.
This feature replaces Unicode glyph placeholders and ad-hoc English string
literals with a small, consistent system so Empty / Loading / actions /
disclosures look and read the same across islands, and so mixed CJK/Latin
content beside icons keeps stable baselines.

Conversation transcripts, tool payloads, Agent display names from hostd, and
user-typed Composer text remain content — they are not rewritten by this
feature.

## Layout

No new island or dock is introduced. The feature only changes marks and copy
inside the existing anatomy:

```text
title bar / canvas
┌──────────────┬─────────────────────────┬──────────────┐
│ Sessions     │ Timeline                │ Agents       │
│ [+]          │ [empty icon + copy]     │ [empty…]     │
│              │                         ├──────────────┤
│              │ Composer                │ Tree         │
│              │ Activity ▾ · Send       │ ▸ / ▾ rows   │
└──────────────┴─────────────────────────┴──────────────┘
status bar · connection · usage
```

- Island headers keep title text; header actions may use icons (for example
  New Session uses a Plus mark instead of a `+` character).
- Empty and Loading placeholders keep a centered mark above title and optional
  subtitle; the mark becomes a sized icon, not a large Unicode character.
- Tree and Activity disclosures keep a fixed-width column; the control becomes
  a chevron icon instead of `▸` / `▾`.
- Status, role, and connection continue to use the existing 6 px color markers.
  Those markers are not icons.

## Behavior and interactions

### Icons

- Chrome actions, Empty / Loading marks, and disclosures draw from one small
  icon set with fixed sizes aligned to the type scale.
- Locked Empty / Loading marks and row marks for v1:

  | Surface | Icon |
  |---|---|
  | Sessions empty / center no-session | `Circle` |
  | Sessions directory (collapsed / open) | `Folder` / `FolderOpen` |
  | Sessions session row | `MessageSquare` |
  | Timeline empty | `MessageSquare` |
  | Timeline user / assistant | `User` / `Bot` |
  | Agents empty / agent row | `Bot` |
  | Tree empty | `Network` |
  | Tree user / assistant / tool | `User` / `Bot` / `Wrench` |
  | Tree model / thinking / branch / compaction | `Cpu` / `Brain` / `GitBranch` / `Layers` |
  | Tree other | `Circle` |
  | Loading (any island) | `CircleDashed` |
  | Error placeholder | `TriangleAlert` |
  | New Session action | `Plus` |
  | Disclosure collapsed / expanded | `ChevronRight` / `ChevronDown` |

- Disclosure hits remain separate from row activation.
- Icons tint with the same muted / foreground / accent / danger colors already
  used by chrome; they do not introduce a second palette.
- Reduced-motion preference does not remove icons; it only affects decorative
  motion already covered by the Workbench contract.

### Typography

- Interface chrome uses named type roles rather than one-off sizes:
  metadata, control/tree label, conversation body, and placeholder emphasis.
- UI chrome uses the system UI font (including mixed CJK). Monospace remains
  reserved for code, logs, ids, and tool detail.
- Icon boxes share the same vertical metrics as their adjacent label role so
  Latin and CJK baselines stay aligned at 100% and Retina scaling.
- Reading width and island density rules from the UI Guidelines stay in force.

### Localization

- All Workbench chrome strings that the GUI owns move behind stable catalog
  keys: island titles, Empty / Loading copy, tooltips, Composer chrome
  (placeholder, Send, Stop), **all Activity summary and item label templates**,
  StatusBar connection labels, Sheet titles, and dialog chrome (Submit, Cancel,
  Decline, section labels such as Arguments).
- v1 ships **English (`en`) only**. The catalog and call sites are structured so
  additional locales can be added later without rewriting islands.
- There is no locale override, OS-locale following, or language picker in this
  wave. The runtime locale is always `en`.
- Hostd and protocol content stay as received: transcript Markdown, tool
  arguments/results, Agent names, model ids, error payloads from the host, and
  user drafts are not translated by the GUI. Chrome may wrap a host fragment
  with an English template (for example `Error: %{message}`).

### Persistence

- No new `[gui]` keys in this wave (locale override is deferred with
  multi-locale support).
- Type roles and icon choices are not user-configurable in this feature.

## Configuration

No new settings keys. Existing Workbench shortcuts keep their physical
bindings; visible labels stay English via the `en` catalog.

## In scope

- Vendored icon subset for Workbench chrome with the locked Empty / Loading set
- Replacement of Unicode placeholder / disclosure / header-action glyphs
- Named typography roles wired through island chrome and shared widgets
- English chrome catalog (`en`) covering all GUI-owned chrome, including every
  Activity summary / item template
- Updates to the UI Guidelines and manual UX checklist for icons and type

## Non-goals

- Shipping `zh-CN` or any non-`en` chrome locale in this wave
- `[gui].locale`, OS-locale following, or an in-app language picker
- Translating Session transcript, tool I/O, Agent display names, or host errors
- A Settings UI (Settings remain deferred)
- Light theme or user-installable icon packs
- Full Lucide / Material catalogs or emoji as the primary icon medium
- Replacing 6 px status / role / connection markers with icons
- TUI string localization or shared GUI/TUI string crates in this wave
- Custom illustration sets beyond Empty / Loading marks
- VoiceOver / accessibility announcement bridging (still a known limitation)

## Acceptance (user-visible)

- Empty and Loading states show the locked icon marks and English catalog
  titles / subtitles.
- Tree and Activity disclosures use chevron icons with stable column width.
- New Session and other icon-capable chrome actions no longer rely on bare
  `+` / glyph characters.
- Activity summaries and item labels resolve through catalog templates (no
  leftover hardcoded English literals in Activity chrome).
- Mixed CJK/Latin labels beside icons remain vertically centered on Retina.
- StatusBar connection marker and Timeline / Activity status markers remain
  color dots, not icons.
