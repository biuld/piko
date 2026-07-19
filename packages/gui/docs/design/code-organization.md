# GUI Code Organization Design

> Status: normative / migrated (M1–M7 landed; physical `platform/` grouping deferred)
> Parent: [GPUI Desktop Client Design](overview.md)
> Related: [GUI Primary Surface Design](primary-surface.md),
> [Settings Ownership Design](../../../../docs/settings-ownership-design.md),
> [Host Command Catalog Design](../../../../docs/host-command-catalog-design.md),
> [GUI Overlay Stack Design](overlay-stack.md)
> Inspiration (not copy): VS Code workbench vs contributions, JetBrains tool windows,
> feature-sliced frontends, TUI Slot/Panel/Component (`packages/tui/AGENTS.md`)

## 1. Purpose

Redefine `packages/gui` module boundaries so that:

1. **Shell chrome** never owns product business UI.
2. **Features** are vertical slices (UI + local VM + feature actions).
3. **App** is a thin composition root (wiring, not a junk drawer).
4. **Platform** adapters (host bridge, transport, theme, i18n, config schema)
   stay at the edges.
5. New work (Settings, Palette, Auth, MCP…) has an obvious home.

This document is the code-organization contract. Visual rules stay in
`ui-guidelines.md`; product behavior stays in feature docs.

## 2. Problems with the current tree

```text
app/          ← composition + too many feature actions mixed together
chrome/       ← shell + Workbench layout + Settings sections + Palette UI
islands/      ← Workbench product features (good pattern)
overlays/     ← prompt bodies (good pattern, inconsistent naming)
projections/  ← shared VMs
config/ theme/ i18n/ bridge/ transport/
```

Concrete failures:

| Smell | Example |
|---|---|
| Product UI inside shell | `chrome/settings/sections/*` (model/auth/sandbox forms) |
| Shell owns feature tools | Command Palette body under `chrome/overlay/palette` |
| Inconsistent product homes | Islands vs overlays vs settings-in-chrome |
| Fat `app/` | settings/palette/quit/island/composer hosts all siblings |
| IA enums in shell | `SettingsSection` in `chrome/primary_surface.rs` |
| Unclear dependency direction | Settings render reaches into `DesktopApp` methods freely |

Primary Surface and Overlay stack concepts are fine; **folder ownership** drifted.

## 3. Industry principles (applied to piko)

1. **Shell vs contributions** (VS Code / Fleet-like): the workbench shell lays
   out surfaces; features contribute content into slots.
2. **Feature modules** (feature-sliced design): a feature owns its UI, view
   models, and user intents; shared kits stay dumb.
3. **Composition root** (Clean Architecture): `app` wires dependencies; it does
   not become the feature catalog.
4. **Dependency rule**: features may use shell primitives and platform APIs;
   shell must not depend on feature internals.
5. **One pattern for product surfaces**: Workbench islands, Settings, prompt
   bodies, and Palette are all **features** mounted by shell hosts.
6. **Align with TUI vocabulary where useful**: TUI’s Slot / Panel / Component ≈
   GUI’s Shell slot / Feature surface / shared UI kit — same idea, different
   toolkit.

## 4. Target layers

```text
┌─────────────────────────────────────────────────────────────┐
│ app/                 Composition root (DesktopApp, bindings) │
├─────────────────────────────────────────────────────────────┤
│ features/            Product vertical slices                 │
│   workbench islands, settings, palette, prompts, …           │
├─────────────────────────────────────────────────────────────┤
│ shell/               Window chrome & surface hosts only      │
│   primary surface, workbench layout, overlay host, island    │
│   panel shell, title/status frame slots                      │
├─────────────────────────────────────────────────────────────┤
│ platform/            Edges: bridge, transport, config,       │
│                      theme, i18n, assets                     │
├─────────────────────────────────────────────────────────────┤
│ projections/         Shared Client Core → VM mappers         │
│                      (prefer feature-local VMs over time)    │
└─────────────────────────────────────────────────────────────┘
```

### 4.1 Dependency direction (must hold)

```text
app ──► features ──► shell
                 └► platform
                 └► projections (transitional)

shell ──► platform/theme (tokens, metrics, island())
shell ✗ features     # forbidden

features ✗ each other’s internals (prefer app routing / small public msgs)
platform ✗ features / shell UI
```

`piko-client-core` remains outside GUI folders; GUI talks to it only through
`platform/bridge` (today’s `bridge/`).

## 5. Target tree

```text
packages/gui/src/
├── main.rs
├── cli/
├── assets.rs
│
├── app/                          # composition root ONLY
│   ├── mod.rs
│   ├── desktop_app.rs            # owns Entities, PrimarySurface, OverlayHost
│   ├── bindings.rs               # keybindings / actions registration helpers
│   ├── layout_state.rs           # Workbench dock prefs (or move under features/workbench)
│   └── wiring/                   # optional: thin adapters that call feature APIs
│       ├── island_routing.rs     # IslandMsg fan-in (today island_dispatch/actions)
│       ├── overlay_sync.rs       # prompt queue → OverlayHost
│       └── surface_nav.rs        # PrimarySurface open/close (today primary_surface_nav)
│
├── shell/                        # rename from chrome/ — name matches role
│   ├── mod.rs
│   ├── primary_surface.rs        # enum Workbench | Settings { section_id }
│   ├── workbench/                # layout ONLY (assemble columns, gutters)
│   │   ├── frame.rs              # TitleBar slot + body + StatusBar slot
│   │   ├── assemble.rs
│   │   ├── title_bar.rs          # dock toggles + gear affordance (no Settings forms)
│   │   └── status_bar.rs
│   ├── settings_frame/           # Settings surface SHELL only
│   │   ├── frame.rs
│   │   ├── title_bar.rs
│   │   └── body_slots.rs         # nav slot + panel slot (no section forms)
│   ├── overlay/                  # stack, priority, Escape, backdrop, panel chrome
│   │   ├── host.rs
│   │   ├── kinds.rs
│   │   └── surface.rs
│   ├── island/                   # IslandPanel shell (header/viewport/focus ring)
│   └── widgets/                  # shell-wide primitives (tree_list row chrome, …)
│
├── features/
│   ├── mod.rs
│   ├── sessions/                 # was islands/sessions
│   ├── timeline/
│   ├── composer/                 # includes Activity
│   ├── agents/
│   ├── tree/
│   ├── settings/                 # PRODUCT settings (moved out of chrome)
│   │   ├── mod.rs
│   │   ├── section.rs            # SettingsSection IA enum + i18n keys
│   │   ├── nav.rs
│   │   ├── sections/
│   │   ├── widgets.rs            # settings form controls (not shell widgets)
│   │   ├── actions.rs            # ConfigUpdate / Intent helpers (was app/settings_actions)
│   │   └── render.rs             # panel body entry
│   ├── palette/                  # Command Palette UI + local command merge
│   │   ├── mod.rs
│   │   ├── render.rs
│   │   ├── catalog.rs            # host ⊕ local merge, runnable rules
│   │   └── actions.rs            # was app/palette_actions (palette-specific)
│   ├── prompts/                  # was overlays/ — approval + interaction bodies
│   │   ├── approval.rs
│   │   ├── interaction.rs
│   │   └── coordinator.rs
│   └── quit/                     # busy-quit confirm body + policy (optional split)
│
├── projections/                  # shared mappers (shrink over time)
├── platform/                     # optional rename grouping; may keep flat crates paths
│   ├── bridge/                   # or keep top-level bridge/ during migration
│   ├── transport/
│   ├── config/                   # GuiSettings + HostRuntimeSettings schemas
│   ├── theme/
│   └── i18n/
└── locales/
```

During migration, **top-level `bridge/`, `transport/`, `theme/`, `config/`,
`i18n/` may remain un-prefixed** to avoid a mega-move. The *logical* layer is
still “platform”; physical `platform/` folder is optional Phase B.

## 6. What belongs where (decision table)

| Concern | Home | Not home |
|---|---|---|
| PrimarySurface enum / frame switch | `shell` + `app/wiring/surface_nav` | features |
| Workbench column layout / resize | `shell/workbench` | features |
| IslandPanel / focus ring | `shell/island` | features |
| Overlay stack / Escape policy | `shell/overlay` | features |
| Sessions/Timeline/… UI + VM | `features/<name>` | `shell` |
| Settings section forms / IA | `features/settings` | `shell` |
| Command Palette list/search/submenu | `features/palette` | `shell/overlay` |
| Approval / interaction forms | `features/prompts` | `shell` |
| `[gui]` / host runtime schema | `config` (platform) | features UI |
| ConfigUpdate / SetModel calls | `features/settings/actions` or small `app/wiring` | `shell` |
| Theme tokens / typography | `theme` | features (consume only) |
| Client Core projection | `bridge` + `projections` / feature VM | `shell` |

## 7. Feature module contract

Each `features/<name>` should expose a small public surface:

```text
features/<name>/
  mod.rs          # pub render / Entity / messages
  view.rs         # GPUI Entity when stateful (islands)
  vm.rs           # view model (pure where possible)
  render.rs       # IntoElement builders
  actions.rs      # optional: feature-local mutations via bridge handles
```

**Messaging:**

- Prefer feature-local messages (today `IslandMsg`) routed by `app/wiring`.
- Features must not import sibling feature internals.
- Cross-feature needs go through `DesktopApp` wiring or shared projections.

**Settings special case:**

- v1 may remain function-based render (no Entity) mounted into
  `shell/settings_frame` slots.
- Still lives under `features/settings`, never under `shell`.

**Palette special case:**

- UI Entity/state under `features/palette`.
- `shell/overlay` only mounts it as a Transient layer and owns Escape order.

## 8. App composition root contract

`DesktopApp` should read as:

1. own platform handles (`ClientBridge`) and shell hosts (`OverlayHost`,
   `PrimarySurface`);
2. own feature Entities (islands, palette, prompt forms);
3. register actions/keybindings;
4. poll/apply bridge updates → refresh feature VMs;
5. **not** contain long feature-specific form logic (those move to
   `features/*/actions.rs`).

`app/` file count should shrink: migrate `settings_actions`, palette-specific
handlers, quit-busy body policy toward features; keep only routing.

## 9. Naming

| Avoid | Prefer |
|---|---|
| `chrome` as catch-all | `shell` (window chrome + hosts) |
| `overlays` vs `chrome/overlay` confusion | `shell/overlay` + `features/prompts` |
| Settings “in chrome because Primary Surface” | Settings **feature** + settings **frame** shell |
| `widgets` meaning everything | `shell/widgets` vs `features/<x>/widgets` |

Keep user-facing product name “Workbench”; keep code name `shell/workbench` for
the session layout surface.

## 10. Migration sequence (incremental, keep green)

Do not big-bang rename the crate in one PR if avoidable.

| Step | Move | Risk |
|---|---|---|
| **M1** | Extract `features/settings` from `chrome/settings/sections|nav|widgets` + `app/settings_actions`; leave `chrome/settings` as frame-only | Low–med |
| **M2** | Move `SettingsSection` to `features/settings/section.rs`; shell stores opaque/current section only via re-export | Low |
| **M3** | Move Palette module to `features/palette`; overlay mounts it | Med |
| **M4** | Rename `overlays/` → `features/prompts` | Low |
| **M5** | Rename `islands/` → `features/{sessions,…}` (or `features/workbench/…`) | Med (imports) |
| **M6** | Rename `chrome/` → `shell/` | Med (imports) |
| **M7** | Slim `app/`: wiring submodules only | Low |
| **M8** | Optional: physical `platform/` grouping | Low |

After M1, the original Settings layering bug is fixed even if renames lag.

## 11. Enforcement

1. Document this file as the organization contract; link from `gui-design.md`.
2. Code review rule: **no new product forms under `shell/` / `chrome/`**.
3. Optional later: `clippy` / CI path allowlists (e.g. deny `chrome/settings/sections`).
4. File size rules from `AGENTS.md` still apply; features split by section, not
   by dumping into shell.

## 12. Non-goals

- Merging GUI into Client Core
- Pixel redesign (ui-guidelines unchanged)
- Matching TUI directory names literally (`panels/`)
- Multi-window / plugin system
- Immediate move of every `projections` helper into features

## 13. Open questions

1. Islands under `features/sessions` flat vs `features/workbench/sessions`?
   **Recommendation:** flat `features/<island>` — Workbench is a shell layout,
   not a feature parent.
2. Keep directory name `chrome` until M6 to reduce churn, with docs saying
   “chrome ≡ shell”? **Recommendation:** docs say shell; rename when M6 lands.
3. Should `layout_state` live under `shell/workbench` or `app`?
   **Recommendation:** `shell/workbench/layout` (already mostly there) + app
   only persists `[gui]` via platform config.

## 14. Summary

| Layer | One-liner |
|---|---|
| **shell** | Where the user *is* framed (surfaces, docks, overlay host, island chrome) |
| **features** | What the user *does* (session, chat, settings, palette, prompts) |
| **app** | How pieces are wired |
| **platform** | How we talk to hostd and draw tokens |

Fix Settings first (M1): product settings leave chrome; chrome keeps only the
Settings Primary Surface frame.
