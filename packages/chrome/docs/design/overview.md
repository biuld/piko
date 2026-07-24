# Design overview

> Status: normative — **how the kit is layered**  
> Audience: chrome maintainers and deep integrators  
> Features (what apps integrate): [../features/](../features/)  
> Roadmap: [../roadmap/](../roadmap/)  
> Related: [archipelago.md](archipelago.md), [island-interaction.md](island-interaction.md),
> [AGENTS.md](../../AGENTS.md)

## 1. Mission

This crate is a **GPUI Islands UI infrastructure kit**, not a product shell.

It exists so a multi-pane desktop app can:

1. switch **exclusive full-frame places** (archipelagos);
2. host **islands** with isolated render + focus ownership;
3. route **chrome-level** navigation/focus without product domain types;
4. paint **shared surfaces**, **lists/trees**, **overlays** with consistent density;
5. stay free of product hosts, session kernels, protocol crates, and i18n catalogs.

**The app owns:** product ids, domain messages, data projection, backend bridges,
which workspace trees exist, what “activate this row” means in domain terms.

**This kit owns:** reusable runtime contracts and presentational primitives that
any second multi-pane GPUI app would need **without** a specific product model.

When published externally, the kit must remain understandable with **no**
references to any particular product monorepo layout.

## 2. Capability map (layers)

```text
┌─────────────────────────────────────────────────────────────┐
│ L4  Presentational kit     src/components/markdown           │
│     Markdown · src/theme · src/assets                        │
├─────────────────────────────────────────────────────────────┤
│ L3  Composite components   src/components/{panel,overlay,list}│
│     IslandPanel · Overlay · ListKeyboard · tree paint        │
├─────────────────────────────────────────────────────────────┤
│ L2  Island runtime         src/runtime/island                │
│     IslandView · FocusRing · FocusTable · FocusMsg · defer   │
├─────────────────────────────────────────────────────────────┤
│ L1  Archipelago runtime    src/runtime/{archipelago,layout}  │
│     Router · Workspace · IslandNode · ChromeRoute            │
└─────────────────────────────────────────────────────────────┘
```

The public API mirrors this tree through `runtime`, `components`, `theme`, and
`assets`. There are no crate-root type re-exports or compatibility facades.

### L1 — Archipelago runtime

| Capability | API (illustrative) | Status |
|---|---|---|
| Exclusive place router | `ArchipelagoRouter`, transitions | **shipped** |
| Workspace declaration | `ArchipelagoWorkspace` (tree + focus_order) | **shipped** (apps read workspace for tree + Tab order) |
| Chrome routing | `ChromeRoute`, `ArchipelagoMessage`, `route_chrome_message` / `route_archipelago_nav` | **shipped** (product path uses route API) |
| Frame slot composition | TitleBar/StatusBar slots | **app** (kit only documents) |

### L2 — Island runtime

| Capability | API | Status |
|---|---|---|
| Island panel chrome | `IslandPanel`, body states, viewport | **shipped** |
| Focus ownership vs caret | `FocusReason`, `FocusRing`, `IslandView` | **shipped** |
| Heterogeneous focus table | `IslandFocusTable`, `try_focus`, `assert_covers` | **shipped** |
| Deferred host messaging | `IslandHost`, `schedule_island_message` | **shipped** |
| Focus message layer | `FocusMsg`, `IslandMessage` | **shipped** |

### L3 — Composite widgets

| Capability | API | Status |
|---|---|---|
| Overlay geometry | `overlay_envelope`, `render_overlay_layer` | **shipped** |
| Overlay focus lifecycle | `OverlayFocusSession` | **shipped** (contract; app wires open/close) |
| Flat list keyboard state | `ListKeyboard` + intents/effects | **shipped** |
| Tree row paint | `TreeRowSpec`, `render_tree_list`, `keyboard_focused` | **shipped** |
| Tree expand keyboard | effect `ToggleExpand` on `ListKeyboard` | **shipped** (pure; app maps to domain) |
| Selectable nav list paint | `ListRowSpec` / `render_list` / `list_row_chrome` | **shipped** |
| TreeList composite flags | `tree_row_chrome` | **shipped** |
| Overlay focus session | `OverlayFocusSession` + host begin/end | **shipped** (app wires open/close) |
| Modal focus trap (Tab cycle) | full containment | **gap** (platform-dependent) |

### L4 — Presentational kit

| Capability | API | Status |
|---|---|---|
| Semantic tokens / density | `tokens`, `metrics`, `TextRole`, `ThemeSnapshot` | **shipped** (dark + light palettes) |
| Icons | icon enum + `ChromeAssets` | **shipped** |
| Native Markdown | opaque document + parse/render functions | **shipped** |
| Domain role colors (chat authors, …) | app extension (not chrome core) | **shipped** boundary — apps own domain roles |

## 3. What this kit deliberately does **not** provide

| Non-goal | Why |
|---|---|
| Product archipelago ids | App vocabulary |
| Domain message variants | Backend / product host |
| Product settings IA / forms | Product feature |
| Command catalogs / prompt policies | Product + protocol |
| Dock-fit product constants | App layout policy (optional later as *configurable* policy) |
| i18n string catalogs | App locales |
| Session kernels / host processes | Outside UI kit |

If a feature only makes sense for one product domain, it is **not** chrome.

## 4. Contracts apps must implement

Minimum viable app on this kit:

1. **Archipelago ids** + `ArchipelagoRouter` (or equivalent enter/leave).
2. Per island workspace: **`IslandView` entities** + `IslandFocusTable` +
   `assert_covers(focus_order)`.
3. **Host** implements `IslandHost` and defers with `schedule_island_message`.
4. Product message enum implements `IslandMessage` / `ArchipelagoMessage` for
   chrome routes; domain arms stay local.
5. **List islands** hold `ListKeyboard`, map `ListKeyEffect` → product intents;
   do **not** reimplement cursor wrap.
6. Overlays: pass **viewport** into `OverlayPanelSpec`; use
   `OverlayFocusSession` on open/close with island focus restore.

## 5. Infra gaps (priority)

Tracked in **[roadmap/README.md](../roadmap/README.md)** (epics A–F):

| P | Feature IDs | Deliverable |
|---|---|---|
| P0 | A2, A3 | Workspace + chrome route on the real app path — **done** |
| P0 | D5 | Apps consume `ListKeyboard` only — **done** |
| P1 | D3, D4 | Selectable list + TreeList composite contracts — **done** |
| P1 | E4 | App host wires `OverlayFocusSession` + focus restore — **done** |
| P1 | A1, A4, E5 | Semantics, secondary islands, overlay viewport — **done** |
| P1 | B3 | `FocusTransition { from, to }` — **done** |
| P1 | F1–F4 | Theme snapshot + dark/light + domain split — **done** |
| optional | C3, D6, E6 | restore_kind / a11y roles / Tab trap |

## 6. Decision rules for new code

**Add here when:**

- a second multi-pane GPUI app would copy-paste the same logic;
- the type is parameterized by app ids or is pure UI mechanics;
- tests run without product hosts or protocol crates.

**Keep in the app when:**

- names product entities (sessions, agents, turns, approvals, …);
- requires a product bridge or product i18n;
- is a one-off product layout constant.

**Anti-pattern:** implementing keyboard / focus / routing again inside app
feature modules when this kit already (or should) own the mechanic.

## 7. Relationship to consuming apps

Consuming applications are **not** part of this crate:

- Prove APIs with unit tests and optional external samples — not by importing a
  product monorepo.
- Prefer migrating apps onto chrome controllers over growing app-local copies.
- Do **not** grow application feature complexity as a substitute for missing
  chrome APIs.

## 8. Summary

> Chrome = archipelago + island runtime + composite widgets + visual system.  
> App = product graphs, domain messages, and content that fills islands.
