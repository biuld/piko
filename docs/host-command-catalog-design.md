# Host Command Catalog and Palette Design

> Status: Phase 1 implemented — protocol (`HostCommandDescriptor` /
> `HostCommandInvoke` / `HostCommandGroup`), hostd neutral catalog, GUI
> palette merge (host + local), and TUI slash/palette adapter are in place.
> Remaining work: GUI flows for Args/Confirm host ids (rename, import,
> export, delete, fork, clone, compact, login, logout) and GUI Primary
> Surface Settings (`open.settings` is currently a stub).
> Related: [Settings Ownership Design](settings-ownership-design.md),
> [GUI Archipelago Design](../packages/gui/docs/design/archipelago.md),
> [GUI Overlay Stack Design](../packages/gui/docs/design/overlay-stack.md),
> [GUI Command Palette Feature](../packages/gui/docs/features/command-palette.md) (supersedes catalog assumptions)
> Protocol: `packages/protocol` (`command_catalog.rs`, `Command` / `Event`)

## 1. Purpose

Redesign the hostd command catalog into a **frontend-neutral product command
set**, and redefine how GUI and TUI Command Palettes consume it.

Today the catalog mixes host actions, TUI presentation commands, menu openers,
and slash-metadata. That blocks a clean GUI palette and couples protocol to
one frontend.

## 2. Problem

Current `CommandCatalogItem` / `CommandCatalogAction` (`packages/protocol`):

- Copy like `"Exit the TUI"` on `Quit`
- Presentation actions: `ToggleToolsExpanded`, `ClearNotifications`, `Help`
- UI openers: `Settings`, `Tree`, `Models`, `Thinking` (open menus, not set values)
- Presentation fields on the wire: `slash_name`, `visible_in_palette`
- Flat enum of actions that frontends must special-case for nested UX

GUI already works around this (Models/Thinking as local submenus after catalog
hits). That should become the rule, not the exception.

## 3. Decisions

1. **hostd owns a neutral command catalog** — product/session/runtime intents
   only; no TUI/GUI wording or UI chrome actions.
2. **Frontends own presentation commands** — Open Settings, Quit App, Toggle
   Sidebar, Clear Toasts, slash aliases, help overlays.
3. **Palette = merge** of `host_commands ⊕ frontend_commands`, filtered and
   grouped by the frontend.
4. **Settings is not a catalog dump** — open Settings is a frontend navigation
   command; editing prefs uses Config CRUD.
5. **Model / thinking selection UX is frontend-owned**; committing a choice
   calls a host **set** intent (or ConfigUpdate), not a host "open Models menu"
   action.
6. **Protocol catalog DTOs are redesigned** to drop slash/palette presentation
   fields and to express invocation kind without frontend UI types.
7. TUI keeps slash commands as a **TUI mapping layer** onto host + local
   commands; slash strings leave the host catalog wire format.

## 4. Responsibility split

| Owner | Owns | Does not own |
|---|---|---|
| hostd catalog | Neutral, runnable product commands and their argument contracts | Bindings, slash names, palette visibility, menu widgets |
| protocol | Catalog DTO + list command/event | Frontend grouping labels |
| GUI palette | Merge, search, group, nested pickers, bindings (`Cmd+Shift+P`) | Inventing host semantics |
| TUI palette / slash | Merge, `/` aliases, HierarchicalMenu where needed | Writing TUI-only actions into hostd |
| Client Core | Forward catalog fetch; map confirmations to intents/commands | Catalog authorship |
| Settings UI | Config CRUD + auth entry points | Being the command registry |

## 5. Host catalog content (target)

### 5.1 Include (neutral)

Session:

- `session.new`
- `session.fork`
- `session.clone`
- `session.rename` (args)
- `session.delete` (confirm policy is frontend; host executes delete)
- `session.import` (args)
- `session.export` (result: path or payload — host fact)

Auth:

- `auth.login` (optional provider arg)
- `auth.logout` (optional provider arg)

Agent / runtime:

- `session.compact`
- `agents.specs.list` (if catalog-exposed; else keep as dedicated `AgentSpecList`)
- `status.snapshot` only if it returns a **host status fact**, not "open Status panel"

Model / thinking (set, not browse UI):

- `model.set` `{ provider, model }`
- `thinking.set` `{ level }`

Optional discoverability helpers (data, not menus):

- `model.list` — already a dedicated protocol command; palette may call it
  without a catalog "Models" opener
- Same for thinking levels: frontend-constant or host-advertised enum later

### 5.2 Exclude from host catalog

| Item today | Where it goes |
|---|---|
| `Quit` / Exit the TUI | GUI: quit app; TUI: exit process — local |
| `Help` | Frontend help overlay |
| `Settings` | GUI: Settings Archipelago; TUI: local settings menu |
| `Tree` | TUI panel / GUI focus Tree island — local |
| `Sessions` as "open list" | GUI focus/dock Sessions; TUI panel — local |
| `ToggleToolsExpanded` | Frontend presentation |
| `ClearNotifications` | Frontend |
| `Models` / `Thinking` openers | Frontend pickers → `model.set` / `thinking.set` |
| Slash-only synonyms | TUI slash map |

### 5.3 Invocation kinds

Host catalog entries declare how they run, not which widget to open:

```text
enum HostCommandInvoke {
  Immediate,           // no args — fire host command / intent
  Args { schema },     // needs arguments (rename title, login provider, …)
  Confirm,             // frontend must confirm, then fire
}
```

Nested pickers (model list, thinking levels) are **not** invoke kinds. The
frontend opens a submenu, then issues `Immediate`/`Args` set commands.

## 6. Protocol redesign

### 6.1 Replace `CommandCatalogItem` shape

Current (legacy):

```text
CommandCatalogItem {
  id, title, detail,
  action: CommandCatalogAction,  // large UI-coupled enum
  slash_name,
  visible_in_palette,
}
```

Target:

```text
HostCommandDescriptor {
  id: String,              // stable, dotted, e.g. "session.new"
  title: String,           // neutral English product title
  detail: String,          // neutral description
  invoke: HostCommandInvoke,
  // optional: group hint for frontends that want host-suggested grouping
  group: Option<HostCommandGroup>,  // Session | Auth | Runtime | Model | …
}
```

Remove from protocol catalog:

- `slash_name`
- `visible_in_palette`
- Open-menu style `CommandCatalogAction` variants (`Models`, `Settings`,
  `Tree`, `Help`, `Quit`, `ToggleToolsExpanded`, `ClearNotifications`, …)

### 6.2 How the client runs a catalog row (decision: id-only)

The catalog tells the UI **what commands exist**. Something still has to turn
“user confirmed `session.new`” into a real `Command::…` / `ClientIntent`.

Two ways to put that binding on the wire:

| Style | Catalog carries | Who maps to protocol Command |
|---|---|---|
| **A. Id-only (chosen)** | `id` + title/detail/invoke metadata | Client (GUI/TUI/Core) has a table `id → build Command` |
| **B. Effect enum** | A typed `effect: NewSession \| SetModel {..} \| …` on every row | Deserialize enum and dispatch; little/no id table |

**We choose A.** Reasons:

- Catalog stays a discovery/documentation list; execution APIs already exist
  (`SessionCreate`, `SessionCompact`, `ConfigUpdate`, …).
- Adding a command does not require growing a protocol enum for palette rows.
- Frontends can map the same id differently only for *local* presentation, not
  for host semantics.
- Avoids a second parallel taxonomy next to existing `Command` variants.

Concrete shape: `HostCommandDescriptor { id, title, detail, invoke, group? }`.
No `CommandCatalogAction` / `HostCommandEffect` on the catalog DTO. Client Core
(or each frontend) maintains `fn dispatch_host_command(id, args)`.

Legacy `CommandCatalogAction` is deleted after both frontends migrate.

### 6.3 Wire commands / events

Keep:

- `Command::CommandCatalogGet`
- `Event::CommandCatalogListed { commands }`

Optionally add later (not required for v1 of this redesign):

- Catalog version / etag if caching becomes painful
- `HostCommandGroup` as a closed enum in protocol

Breaking change: GUI and TUI catalog consumers update together with protocol.
No long dual-catalog forever; short compatibility shim inside hostd is
acceptable for one release if needed.

### 6.4 Relationship to other protocol APIs

| Need | Mechanism |
|---|---|
| List models | `ModelList` (existing), not catalog opener |
| Read/write settings | `ConfigGet` / `ConfigUpdate` |
| Compact | `SessionCompact` and/or catalog `session.compact` |
| Submit turn | Not a palette catalog item (Composer) |

Palette may *trigger* these; it does not redefine them as menu actions.

## 7. Frontend palette design

### 7.1 Shared product rules

Both palettes:

1. Fetch host catalog.
2. Union with frontend-local commands.
3. Search over title/detail/id (and TUI slash aliases locally).
4. On confirm:
   - host id → map to intent/command
   - local id → local handler (navigate, toggle, quit)
5. Arg-required / confirm-required → frontend collects UI, then executes.

### 7.2 GUI Command Palette

- Remains Overlay **Transient** (`Cmd+Shift+P`); not an Archipelago.
- Local commands include at least:
  - `open.settings` → Settings Archipelago (also TitleBar trailing gear)
  - focus/dock Sessions, Agents, Tree
  - quit app (with existing busy-quit confirm)
- Models / Thinking: keep nested pickers; confirm → `model.set` /
  `thinking.set` (or existing ClientIntent equivalents).
- Remove dependence on host `Models`/`Thinking` opener actions.
- Grouping is GUI-owned (e.g. Session, Model, Settings, Window).

### 7.3 TUI Command Palette / slash

- Local map: `/quit`, `/help`, `/settings`, `/tree`, `/tools`, `/clear`, …
- Host map: `/new`, `/compact`, `/login`, … → host ids
- Slash strings live only in TUI (and docs), not in `HostCommandDescriptor`.
- HierarchicalMenu remains a TUI presentation choice for arg flows.

### 7.4 What "Open Settings" means

| Frontend | Behavior |
|---|---|
| GUI | `ArchipelagoId::Settings` via TitleBar gear, `Cmd+,`, or palette |
| TUI | Existing settings menu / panel |

Neither is a hostd command.

## 8. Catalog authorship in hostd

- Single module (today `packages/hostd/src/domain/commands.rs`) emits only
  neutral descriptors.
- Titles/details never mention TUI, GUI, ratatui, or GPUI.
- Tests assert absence of presentation-only ids (`quit`, `help`, `settings`,
  `tools.toggle`, …) in the host list.

## 9. Non-goals

- One shared palette widget between TUI and GUI
- Host-defined keybindings
- Encoding full JSON Schema for every args command in v1 (start with explicit
  known arg kinds)
- Replacing Config CRUD with catalog entries for every setting key
- Multi-language catalog strings in hostd (frontends may re-label by id later)

## 10. Implementation sequence

1. Spec target descriptor + id list (this doc) and freeze exclusions.
2. Protocol: add new DTOs; migrate `CommandCatalogListed`.
3. hostd: emit neutral catalog only; map ids to real handlers.
4. GUI palette: merge local commands; retarget Models/Thinking to set intents;
   add `open.settings`.
5. TUI: slash/palette adapter onto new ids; keep local commands local.
6. Delete legacy `CommandCatalogAction` UI variants when both frontends are
   migrated.
7. Update `packages/gui/docs/features/command-palette.md` and TUI command docs to match.

## 11. Open questions

1. Should `session.delete` require host-side confirm token, or only frontend
   confirm?
2. Do we advertise thinking levels from hostd, or keep them frontend constants?
3. Is `status` a host snapshot command or purely frontend diagnostics?
