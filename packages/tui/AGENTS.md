# AGENTS.md — piko-tui crate context

## Architecture overview

```
piko-tui-layout  →  shell split, flex plane, modal z-stack, FocusManager<T>
piko-tui         →  Region / SurfaceId / AppMode, declare trees, paint, route keys
```

```
Terminal
  → split_shell (BottomBar chrome: agent · model · cwd · ctx · cost)
  → plane      Stream | Notice? | Suggest? | Composer
  → modals     Browse CoverBody | Select/Dock ComposerBand | Modal Centered
  → solve → paint chrome → plane (unless CoverBody) → layers
```

## Information architecture

```text
┌──────────────────────────────────────┐
│ STREAM            (plane, grow)       │  conversation only
├──────────────────────────────────────┤
│ DOCK              (plane, bottom)     │
│  Notice?                              │
│  Suggest?                             │
│  Composer                             │
├──────────────────────────────────────┤
│ CHROME            (shell bottom)      │  agent chip + status
└──────────────────────────────────────┘
        modal z (intent)
        · Browse  CoverBody
        · Select  ComposerBand
        · Dock    ComposerBand (approval / tool workflow replace the composer)
        · Modal   Centered (settings dialog)
```

**Agents** are a Select surface (`SurfaceId::Agents`, F4 / `/agents`) on
ComposerBand — switch the viewed session agent. Chrome shows a compact agent
summary only.

## Surface intent

| Intent | Placement | Surfaces |
|--------|-----------|----------|
| Browse | `CoverBody` | Sessions, Tree, Help, Diagnostics, SummaryPrompt |
| Select | `ComposerBand` | Agents, Models, AuthSelector, MCP, Processes |
| Dock | `ComposerBand` | Approval, ToolInteraction |
| Modal | `Centered` | Settings, Status |

Define via `SurfaceId::intent()` / `modal_layer(body, band_h)`.

Select band height comes from feature **content-row** budgets
(`SelectBandBudget` in `navigation/select_band.rs`), not a fixed body fraction.
Overflow list items scroll.

## Layers ownership

| Concern | Module |
|---------|--------|
| Region leaves | `navigation/region.rs` |
| Surface catalog + intent | `navigation/surface.rs` |
| `compose_plane` / `compose_modals` | `navigation/compose.rs` |
| AppState → solve | `layout/` |
| Paint | `render/` |
| Panels | `features/*` |

## Design rules

1. Plane stays stable; surfaces only add modal layers.
2. No floaters outside solved rects / chrome.
3. Layout crate has no product ids.
4. Focus is LIFO (`AppMode::Chat` | `Surface`).

## TUI config

- `tui.bottom_bar.items` — default includes `agent`

## Docs

Engine: `packages/tui-layout/docs/`. Product: `packages/tui/docs/`.
