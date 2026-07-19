# GUI Primary Surface Design

> Status: Phase 2 implemented (Primary Surface switch + Settings stub frame)
> Parent: [GPUI Desktop Client Design](overview.md)
> Related: [GUI Overlay Stack Design](overlay-stack.md),
> [Settings Ownership Design](../../../../docs/settings-ownership-design.md),
> [Host Command Catalog Design](../../../../docs/host-command-catalog-design.md)
> Feature follow-up: GUI Settings surface (separate feature doc when scoped)

## 1. Purpose

Introduce **Primary Surface** as a first-class chrome concept so the window can
host whole-product experiences that are peers of the Session Workbench —
starting with Settings — without overloading the Overlay stack or the island
tree.

A Primary Surface is not only the center content. It owns the **full chrome
frame** that switches together:

```text
TitleBar + content + StatusBar
```

Overlays / sheets / toasts stay outside the surface and paint above it.

This design also reorganizes `packages/gui/src/shell` so each surface is a
cohesive module (its bars + body), and Workbench is no longer the implicit
definition of chrome itself.

## 2. Problem

Today `DesktopApp::render` always mounts a fixed shell around Workbench body:

```text
TitleBar          ← Workbench-shaped (dock toggles only)
content           ← Workbench islands only
StatusBar         ← Workbench session chrome only
Overlay / Sheet / Toast
```

Consequences:

- Settings cannot be a real product page; it would have to fake an overlay or
  hijack the center column.
- TitleBar / StatusBar are global and Workbench-specific; swapping only the
  middle leaves wrong chrome (dock toggles, turn/cost) on Settings.
- Overlay already owns Prompt / Confirm / Palette; stuffing Settings there
  conflates "go somewhere" with "temporary tool".
- `shell/` is structured as if Workbench *is* the only chrome.

## 3. Decisions

1. **Primary Surface** = the exclusive **frame** below the window and above
   Overlay layers: TitleBar + body + StatusBar, switched as one unit.
2. v1 surfaces: `Workbench` | `Settings`.
3. Surfaces are **peers**. Settings is not an island, not a Workbench center
   replacement, and not an Overlay Transient.
4. Settings uses **full-frame replacement**: when Settings is active, Workbench
   TitleBar/body/StatusBar are not shown. Island Entities remain alive in
   `DesktopApp` for restore.
5. Settings body is its own **two-column** nav + panel. It does not reuse the
   Workbench island tree.
6. Overlay stack stays orthogonal and paints above the active Primary Surface.
7. Motion is optional and deferred: v1 hard-cuts the whole frame; later short
   fades must honor `[gui].reduced-motion`.
8. **Directory:** each surface module owns its TitleBar presentation, body, and
   StatusBar presentation (or explicitly omits StatusBar).
9. **Settings entry:** TitleBar **trailing gear** toggles Settings ↔ Workbench
   (aligned with `island_gutter`); `Cmd+,` same toggle; palette opens only.

## 4. Window layout contract

```text
DesktopApp
├── PrimarySurface                    # exclusive frame (switched together)
│   ├── Workbench
│   │   ├── TitleBar   dock toggles · brand · gear
│   │   ├── Body       Sessions | Timeline+Composer | Agents/Tree
│   │   └── StatusBar  session / context / cost
│   └── Settings
│       ├── TitleBar   Back · brand/title · (no gear / disabled)
│       ├── Body       Nav | Panel
│       └── StatusBar  none (v1) — or surface-owned footer later
└── layers (unchanged, above any surface)
    ├── OverlayHost
    ├── Sheet
    └── Notification
```

| Layer | Role |
|---|---|
| Primary Surface | Where the user *is* — full frame (bars + body) |
| Overlay | Temporary interruption or tool above that place |
| Sheet | Responsive dock for islands, not product pages |

There is **no** global TitleBar/StatusBar that outlives the surface. Shared
visual primitives (brand mark, ghost icon button, metrics) may live in
`chrome/widgets` or `theme/`; **composition** belongs to the active surface.

## 5. State model

```text
enum PrimarySurface {
    Workbench,
    Settings { section: SettingsSection },
}

enum SettingsSection {
    General,
    Account,
    AgentTools,
    ContextReliability,
    Appearance,
    Keyboard,
    Advanced,
}
```

Owned by `DesktopApp` (or a thin chrome navigator), not by Client Core / hostd.

### 5.1 Transitions

| Trigger | Behavior |
|---|---|
| **TitleBar trailing gear** | Toggle `Workbench ↔ Settings` |
| `Cmd+,` | Same toggle as gear |
| Palette `open.settings` | Open if on Workbench; no-op if already Settings |
| Settings nav click | update `section` only |
| Escape (no overlay) | `Settings → Workbench` |
| Overlay Escape | OverlayHost first; does not pop Settings |
| HostPrompt while on Settings | allowed; paints above Settings frame |

On enter Settings: remember focused `IslandId` (and composer focus if needed).  
On leave Settings: restore that focus when returning to Workbench.

There is **no** Back control — dismiss via gear again, `Cmd+,`, or Escape.

### 5.2 Per-surface chrome (not “modes of one bar”)

**Workbench TitleBar**

```text
[ dock toggles ]     piko     [ ⚙ ]
 leading              brand    trailing (right = island_gutter)
```

**Settings TitleBar**

```text
                     piko / Settings     [ ⚙ active ]
 brand                                 trailing (same inset)
```

Trailing gear aligns with the Workbench island content edge
(`metrics.island_gutter` from the window edge).

**StatusBar**

| Surface | StatusBar |
|---|---|
| Workbench | Current session / context / cost bar |
| Settings (v1) | **Omitted** — Settings frame has no status strip |

Do not keep a shared StatusBar and pass `Minimal` / `Hidden` flags into it.
If Settings later needs a footer (e.g. config path hint), that is
**Settings-owned** UI under `features/settings/`, not a Workbench StatusBar mode.

Trailing gear:

- Ghost icon, same size language as dock toggles.
- Present on **both** surfaces; tooltip Settings / Close Settings.
- Icon: `PikoIcon::Settings` (stronger fg when Settings is active).

## 6. Settings body layout (v1)

```text
┌──────────────────────────────────────────────────────────┐
│                     piko / Settings              [⚙]     │
├──────────────┬───────────────────────────────────────────┤
│ ░ nav island │ ░ panel island                            │
│ General      │ header + form (theme surface, gutters)    │
│ Account      │                                           │
│ …            │                                           │
└──────────────┴───────────────────────────────────────────┘
   (canvas gutters; no StatusBar)
```

- Body uses the same `island_gutter` / `island()` surface language as Workbench.
- Left: section list (fixed width island). Right: header + scrollable form.
- Form content caps at `reading_width`. Typography via `TextRole` / `label_text`.
- Body edits Host + `[gui]` only; see
  [Settings Ownership Design](../../../../docs/settings-ownership-design.md).

## 7. Motion policy

- **v1:** instantaneous swap of the entire Primary Surface frame.
- **Later (optional):** ≤200ms fade on the frame; Overlay does not animate with
  it.
- Honor `ux_prefs.allow_motion()` / `[gui].reduced-motion`.
- Interruptible: no animation queue.

## 8. Module reorganization

### 8.1 Target tree

See [GUI Code Organization Design](code-organization.md) for the
normative layout. Current split after Settings extraction:

```text
packages/gui/src/
├── app/primary_surface.rs     # PrimarySurface enum (composition root)
├── features/settings/         # section IA, nav, forms, actions
└── shell/settings/            # frame shell only (title_bar + body_slots)
```

`DesktopApp::render` becomes:

```text
match primary_surface {
  Workbench => chrome::workbench::mount_frame(...),
  Settings  => compose features::settings content into chrome::settings::mount_frame(...),
}
+ overlay / sheet / notification layers
```

### 8.2 What moves / what stays

| Path | Change |
|---|---|
| `chrome/title_bar.rs` | Move into `workbench/title_bar.rs`; Settings gets its own |
| `chrome/status_bar.rs` | Move into `workbench/status_bar.rs` |
| `chrome/workbench/assemble*` | Body only; frame assembled in `workbench/mod.rs` |
| `shell/overlay/*` | Unchanged role; still window-level |
| `chrome/island/*` | Unchanged; Workbench body only |
| `app/desktop_app.rs` | Switch whole frame via `PrimarySurface`; no global bars |

### 8.3 Naming rule

- **Primary Surface** — the active full frame (bars + body).
- **Workbench** — the session surface implementation.
- **Settings surface** — the settings frame implementation.
- Do not call Settings a "sheet".
- Do not describe TitleBar/StatusBar as global chrome with surface-specific
  “modes”; say each surface **owns** its bars.

## 9. Focus and input

| Context | Tab / shortcuts |
|---|---|
| Workbench | Existing island focus cycle |
| Settings | Focus in Settings nav + panel; island Tab cycle suspended |
| Overlay open | OverlayHost policy wins |

`Cmd+Shift+P` remains available on Settings.  
`Cmd+B` / `Cmd+I` exist only while Workbench frame (and its TitleBar) is active.

## 10. Non-goals

- URL router / deep links / browser history
- Stacked Primary Surfaces
- Shared global TitleBar/StatusBar with Minimal/Hidden flags
- Embedding Settings in the Workbench center column
- Replacing Overlay with Primary Surface for Palette / Approvals
- Animating the first implementation
- Migrating TUI onto Primary Surface concepts

## 11. Implementation sequence

1. Introduce `PrimarySurface` + `match` that renders a full Workbench frame
   (refactor existing bars into `workbench/`).
2. Add Settings frame stub (own TitleBar + empty body, no StatusBar).
3. Wire gear / Back / `Cmd+,` / Escape-when-idle + focus save/restore.
4. Land Settings two-column body + sections (settings ownership).
5. Optional motion pass.

## 12. Open questions

1. Persist last `SettingsSection` in `[gui]` or keep session-local only?
2. Should Settings TitleBar center read “piko” or “Settings”?
3. Busy-quit confirm while Settings is showing — expected yes (Overlay above).
