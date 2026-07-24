# Chrome kit documentation

Product-agnostic GPUI Islands chrome infrastructure.

## Document kinds

| Kind | Path | Answers | Audience |
|---|---|---|---|
| **Features** | [`features/`](features/) | **What** the kit provides (capability contracts) | App authors integrating the kit |
| **Design** | [`design/`](design/) | **How** it is structured and wired | Kit maintainers / deep integrators |
| **Roadmap** | [`roadmap/`](roadmap/) | **Status**, priorities, PR slices | Planning / contributors |

Do **not** mix backlog status into feature contracts, or product monorepo paths
into any of these docs.

## Quick start

1. [features/README.md](features/README.md) — capability index  
2. [design/overview.md](design/overview.md) — layers and boundaries  
3. [roadmap/README.md](roadmap/README.md) — what is done / next  

## Index

### Features (contracts)

| Doc | Topic |
|---|---|
| [archipelago](features/archipelago.md) | Exclusive full-frame places (body = islands) |
| [island-runtime](features/island-runtime.md) | Island isolation, focus, directed messaging |
| [list-keyboard](features/list-keyboard.md) | In-island list/tree keyboard |
| [context-menu](features/context-menu.md) | Compact pointer-anchored flat action menu |
| [overlay](features/overlay.md) | Modal/transient panel chrome |
| [notification-surfaces](features/notification-surfaces.md) | Floating notification history presentation |
| [native-markdown](features/markdown.md) | Semantic Markdown to GPUI documents |
| [theme](features/theme.md) | Density, type, color, icons |

### Design (implementation)

| Doc | Topic |
|---|---|
| [overview](design/overview.md) | Layers L1–L4, non-goals, decision rules |
| [archipelago](design/archipelago.md) | Router, workspace, routing order |
| [archipelago-runtime](design/archipelago-runtime.md) | Closure of workspace + route path |
| [island-interaction](design/island-interaction.md) | Message flow, focus Activate/Claimed |
| [island-focus](design/island-focus.md) | FocusTable safety (`try_focus`, assert) |
| [list-keyboard](design/list-keyboard.md) | `ListKeyboard` controller design |
| [context-menu](design/context-menu.md) | Native flat menu state, geometry, focus, and paint |
| [overlay-composite](design/overlay-composite.md) | Envelope + focus session design |
| [notification-surfaces](design/notification-surfaces.md) | Panel and row presentation boundary |
| [markdown-renderer](design/markdown-renderer.md) | Parser adapter, semantic model, GPUI layout |
| [theme-system](design/theme-system.md) | Context theme / palette split design |

### Roadmap

| Doc | Topic |
|---|---|
| [roadmap](roadmap/README.md) | Epic status A–F, PR slices, acceptance rule |

### Crate

| Doc | Topic |
|---|---|
| [AGENTS.md](../AGENTS.md) | Dependencies and coding boundaries |
| [assets/icons](../assets/icons/README.md) | Vendored SVG icons |
