# Pane chrome

> Status: reviewed

## Overview

Pane is the shared overlay chrome for TUI product surfaces: border, title,
optional search, content body, tip, and footer hints. Features only fill
**content** (and choose which zones to enable).

Pane exposes a **complexity mode** and field overrides. Mode describes how
rich the *pane chrome* should be for that surface’s job — not where the
surface is placed in the shell (`CoverBody` / `ComposerBand` remain
navigation / modal placement concerns).

## Layout

### Modes (business complexity)

| Mode | Meaning | Frame | Default padding | Search rule |
|------|---------|-------|-----------------|-------------|
| `Standard` | Multi-zone / complex surfaces | all borders | 1×1 | on (if search shown) |
| `Minimal` | Short pick / simple prompt | top + bottom | 1×0 | off |

Components may override `padding`, `borders`, `search`, `search_rule`,
`tip`, `footer`, `title_affixes` after applying a mode.

### Title affixes (right cluster)

Structured chips; Pane owns paint and spacing. Feature owns *values* (active
index, option labels).

| Affix | Paint | Use |
|-------|-------|-----|
| `Close` | `[x]` | Settings root close cue |
| `Selection { at, of }` | `[n/total]` | List/table cursor |
| `ModeStrip { options, active }` | `A \| [B] \| C` | Scope / filter cycle (mutually exclusive) |

Order in `title_affixes` is left → right within the right-aligned group
(`ModeStrip · Selection · Close`). Multiple independent mode switches →
multiple `ModeStrip` affixes.

Feature example (state stays on the surface; Pane only projects):

```text
options: ["Default","NoTools","User","Labeled","All"]
active:  1  →  Default | [NoTools] | User | Labeled | All
```

Placement (Browse CoverBody vs Select ComposerBand) is **orthogonal**.

### Zones (top → bottom)

1. Title (+ optional right affix)
2. Search / filter (optional)
3. Hairline under search (optional)
4. **Content** (caller paints)
5. Tip (optional)
6. Footer hints or reserved interactive footer (optional)

## Behavior / interactions

- Focused border uses shared feedback tokens (`frame_border_style`).
- Pane does **not** handle keys; focus routing stays in app / feature code.
- Too-small areas degrade: hide chrome first, keep content + hints when possible.

## Who uses which mode

| Surface | Mode | Why |
|---------|------|-----|
| Agents, Models, Auth menu | Minimal | Quick pick, low chrome |
| Auth API-key form | Minimal | Short form prompt |
| Slash suggestions / file browser (Suggest) | Minimal + **no search** | Filter is editor token (`/` / `@`) |
| Settings | Standard | Nested catalog + search rule + hints |
| Sessions, Tree | Standard | Browse table / many affordances |
| Status, MCP, Diagnostics | Standard | Read-heavy info panels |

### Search zone

- Default is **off** (`PaneSearch::Hidden`).
- Turn on with `.search_filter(...)` or `.search(PaneSearch::Shown { … })`.
- Feature can explicitly disable with **`.no_search()`** (also clears `search_rule`).
  Use when filtering is owned elsewhere (editor dock) or the surface does not filter.

## Non-goals

- Encoding modal placement into pane mode
- Decide (approval / tool workflow) chrome in this pass
