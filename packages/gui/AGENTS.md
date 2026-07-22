# AGENTS.md — piko-gui crate context

## Architecture overview

The GUI is a macOS-first GPUI desktop client. It talks to hostd over JSONL
stdio via `piko-client-core`. Shared Islands chrome lives in **`piko-chrome`**
(theme, island panel, generic layout tree, overlay surface, widgets). Module
boundaries follow [GUI Code Organization Design](docs/design/code-organization.md).

```
app        →  composition root (DesktopApp, wiring, ArchipelagoRouter state)
features   →  product vertical slices (UI + local VM + feature actions)
shell      →  piko product shell (Workbench assembly, product OverlayHost, IslandMsg)
piko-chrome→  reusable chrome kit (no product ids / domain messages)
platform   →  edges: bridge, transport, config, theme re-export, i18n, assets
```

```text
piko-gui ──► piko-chrome
piko-chrome ✗ piko-gui | client-core | protocol | hostd
```

TUI vocabulary maps roughly as:

| TUI | GUI |
|---|---|
| Slot | Shell layout slot / dock column |
| Panel | Feature surface (island, Settings panel, overlay body) |
| Component | Shell widget or feature-local control |

### Dependency direction (must hold)

```text
app ──► features ──► shell
                 └► platform (bridge / theme / config / …)
shell ✗ features
features ✗ sibling feature internals
```

Cross-feature needs go through `DesktopApp` wiring or shared projections — not
direct `use` of another feature’s private modules.

### Shell vs features vs chrome

| Concern | Home | Not home |
|---|---|---|
| IslandPanel / FocusRing / body states | **`piko-chrome`** (`crate::theme` / shell re-export) | features |
| Generic split tree / prune | **`piko-chrome::layout`** | product ids |
| Overlay panel geometry | **`piko-chrome::overlay`** | stack kinds |
| Theme tokens / metrics / typography | **`piko-chrome::theme`** (`crate::theme` re-export) | hard-coded sizes in features |
| Tree list row chrome | **`piko-chrome::widgets`** | feature VMs |
| Workbench column layout / resize / dock-fit | `shell/workbench` | features |
| Product `IslandId` / default tree / `IslandMsg` | `shell/island` + `shell/workbench` | **piko-chrome** |
| Overlay stack kinds / Escape / HostPrompt | `shell/overlay` | **piko-chrome** |
| Settings Archipelago frame | `shell/settings` | features |
| Sessions / Timeline / Composer / … | `features/<name>` | shell / chrome |
| Settings section forms / IA | `features/settings` | shell |
| Command Palette UI + local merge | `features/palette` | shell |
| Approval / interaction bodies | `features/prompts` | shell |
| `[gui]` schema | `config/` | feature UI |

**CR rule:** no new product forms under `shell/`.  
**Chrome rule:** no product island ids, domain messages, or host types in `piko-chrome`
(see `packages/chrome/AGENTS.md`).

### App composition root

`DesktopApp` should:

1. own platform handles (`ClientBridge`) and shell hosts (`OverlayHost`,
   `ArchipelagoRouter`);
2. own feature Entities (islands, palette, prompt forms);
3. register actions / keybindings;
4. poll / apply bridge updates → refresh feature VMs;
5. **not** contain long feature-specific form logic (those live in
   `features/*/actions.rs`).

Routing helpers live under `app/wiring/` (`island_*`, `overlay_sync`,
`archipelago_nav`).

### Feature module contract

```text
features/<name>/
  mod.rs          # pub render / Entity / messages
  view.rs         # GPUI Entity when stateful (islands)
  vm.rs           # view model (pure where possible)
  render.rs       # IntoElement builders
  actions.rs      # optional: feature-local mutations via bridge
```

Island → host messaging uses `shell::IslandMsg` with **shell-owned primitive
payloads** (no feature VM types in the enum). Features project VM rows into
those fields at emit time.

**Chrome contracts (required):** each island Entity implements
`piko_chrome::IslandView`; product msgs implement `IslandMessage`; `DesktopApp`
implements `IslandHost`, holds `IslandFocusTable` (`assert_covers` on startup),
and islands emit only via `schedule_island_message` (see
`packages/chrome/src/island/contract.rs`). Interaction model:
[chrome island design](../chrome/docs/design/island-interaction.md).

## Design rules

1. **Shell frames; features fill.** Workbench / Settings / Overlay hosts are
   shell; product content is a feature mounted into slots.
2. **Visual rules live in** [ui-guidelines.md](docs/ui-guidelines.md). Do not
   invent one-off density, type roles, or island chrome.
3. **hostd is authoritative** for sessions, config, prompts, models. GUI keeps
   presentation state (drafts, follow, focus, layout prefs under `[gui]`).
4. **Shared wire types** belong in `piko-protocol`; headless projection in
   `piko-client-core`. GUI must not grow a second session kernel.
5. **File size:** prefer ~300–400 lines per `.rs` file; hard ceiling **500**
   (workspace `AGENTS.md`).
6. **English** for docs and code comments.

## Adding a new product surface

1. Agree the user-visible Feature Doc (see Docs / workflow below).
2. Write a design doc when the change crosses shell/features/app, settings,
   overlay lifecycle, or hostd contracts.
3. Put UI + VM + actions under `features/<name>/`.
4. Mount from shell slots or `app` wiring — do not grow forms inside `shell/`.
5. If the surface needs host intents / config patches, keep mutations in
   `features/<name>/actions.rs` (or a thin `app/wiring` adapter).
6. Validate (`cargo fmt`, `cargo test -p piko-gui`, clippy as needed).
7. Update the Feature Doc so it matches shipped behavior.

## GUI config

GUI prefs live under the `[gui]` section (hostd-stored blob). Schema and
defaults are owned by `packages/gui/src/config/`. See
[Settings Ownership Design](../../docs/settings-ownership-design.md).

## Docs structure

```
docs/
├── features/            # pure user-facing specs ("what")
│   ├── workbench.md
│   ├── command-palette.md
│   └── …
├── design/              # implementation designs ("how")
│   ├── overview.md      # GPUI desktop client design (entry)
│   ├── code-organization.md
│   └── …
├── ui-guidelines.md     # visual / layout system (normative)
├── launch.md
├── known-limitations.md
└── dependency-pins.md
```

Normative architecture entry points:

- [design/overview.md](docs/design/overview.md) — overall GUI design
- [design/code-organization.md](docs/design/code-organization.md) — module boundaries
- [chrome docs](../chrome/docs/README.md) — features / design / roadmap
- [chrome archipelago feature](../chrome/docs/features/archipelago.md)
- [chrome island design](../chrome/docs/design/island-interaction.md)
- [design/archipelago.md](docs/design/archipelago.md) — piko product Workbench ↔ Settings
- [design/overlay-stack.md](docs/design/overlay-stack.md) — overlay priority / Escape
- [features/workbench.md](docs/features/workbench.md) — Workbench product contract

### features/ — functional specs

Describe one feature from the user’s perspective: behavior, layout, shortcuts,
configuration, non-goals. **No** code blocks, file paths, or internal structs.

**Feature doc template:**

```markdown
# Feature Name

## Overview
(one paragraph — what it is, where it lives, what it does)

## Layout
(ascii diagram, placement in Workbench / Settings / Overlay)

## Behavior / interactions
(keyboard shortcuts, open/close, edge cases)

## Configuration
(`[gui]` keys, commands, defaults)

## Non-goals
(what it deliberately does NOT do)
```

### design/ — implementation design

Subsystem architecture: responsibilities, data flow, protocol/config shape,
state ownership, shell vs feature placement, tradeoffs. Code sketches are OK.

**When to write a design doc:** before implementing when the change affects any
of: more than one crate; protocol / hostd commands; `[gui]` schema; Primary
Surface or overlay lifecycle; island focus/messaging; a new shared shell
primitive; multiple features.

### Flow (same idea as TUI)

```
1. Feature Doc / Feature Brief  → product contract ("what")
2. Design doc (if needed)       → contracts / boundaries ("how")
3. Implement                    → follow shell/features/app rules
4. Validate
5. Update Feature Doc           → match stable behavior
```

Feature docs are the source of truth for what the GUI does. Design docs are
the rationale for how it does it. Visual rules stay in `ui-guidelines.md`.

## Validation

```bash
cargo fmt --all
cargo test -p piko-gui
cargo clippy -p piko-gui --all-targets -- -D warnings
```

Use workspace clippy/tests when touching `protocol`, `client-core`, or hostd
settings schemas.
