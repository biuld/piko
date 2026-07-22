# Archipelago Architecture

> Status: normative implementation design  
> Contracts: [`src/archipelago/mod.rs`](../../src/archipelago/mod.rs)  
> Feature: [archipelago](../features/archipelago.md)  
> Related: [island-interaction.md](island-interaction.md)

## 1. Purpose

Define **Archipelago** as a first-class chrome concept aligned with islands:

1. exclusive full-frame place (TitleBar + body + optional StatusBar);
2. **body = a group of islands** (`IslandNode` workspace);
3. **archipelago-level routing** (switch places) above **island-level routing**
   (focus / msgs).

Apps supply product archipelago ids and island ids; chrome owns the mechanism.

**Naming:** an archipelago is a cluster of islands — not a vague “primary
surface.” Overlay stays orthogonal.

## 2. Model

```text
Window
├── ArchipelagoRouter<ArchipelagoId>   # exclusive active archipelago
│   └── ArchipelagoWorkspace
│       ├── frame slots (app: TitleBar / StatusBar)
│       └── IslandNode<IslandId>       # body
│           └── IslandFocusTable + FocusRing
└── Overlay stack (orthogonal, paints above)
```

### 2.1 Archipelago ≠ Overlay

| | Archipelago | Overlay |
|---|---|---|
| Meaning | Where the user *is* | Temporary tool / interruption |
| Frame | Owns TitleBar + body + StatusBar | Panel over dimmed backdrop |
| Body | **Island workspace** | App-provided body element |
| Escape | May leave archipelago (app policy) | App overlay host first |

### 2.2 Archipelago = island workspace

```text
ArchipelagoWorkspace {
  id: ArchipelagoId,
  island_tree: IslandNode<IslandId>,
  focus_order: [IslandId],
}
```

Examples (app-defined, not chrome enums):

| Archipelago (example names) | Islands (examples) |
|---|---|
| Main work surface | Side list, document, inspector |
| Preferences | Nav + detail panel as real `IslandView` entities |
| Future | Preview, diff, debug tools |

**Norm:** do not invent a second layout system for “pages”. Prefer islands so
focus, messages, and prune/dock reuse the same chrome contracts.

Product-only state (e.g. which preferences *section* is open) lives beside the
router, not as a separate chrome archipelago id per section:

```text
router.active = Preferences
app.prefs_section = Appearance   // app field, not ArchipelagoId
```

## 3. Routing layers

```text
Message from island / command
        │
        ▼
ArchipelagoMessage::as_chrome_route()
        │
        ├─ ChromeRoute::Archipelago(ArchipelagoNav)  ──► ArchipelagoRouter
        │
        ├─ ChromeRoute::Island(FocusMsg)             ──► IslandFocusTable
        │
        └─ None  ──► product domain dispatch (app)
```

### 3.1 Archipelago navigation (`ArchipelagoNav`)

| Intent | Behavior |
|---|---|
| `Go { id }` | Hard-cut; clear restore |
| `Enter { id }` | Switch; save previous for leave |
| `Leave` | Restore saved archipelago |
| `TogglePair { a, b }` | Shortcut between two peers |

Mutations return `ArchipelagoTransition` (`Unchanged` | `Changed { from, to }`).
Callers must not remount on `Unchanged`.

Island focus restore on leave is **app policy** (save `FocusRing` when entering,
`activate_archipelago_islands` on leave).

Focus table: use `try_focus` — unknown ids leave the ring untouched.

### 3.2 Island routing (within active archipelago)

See [island-interaction.md](island-interaction.md).

**Norm:** island emit for focus/chrome is ignored when that island’s archipelago
is not active (Entities may still stay alive).

### 3.3 Host dispatch order (required)

```text
1. Overlay Escape / overlay msgs
2. as_chrome_route → Archipelago
3. as_chrome_route → Island focus
4. product domain
```

## 4. Contracts (API)

| API | Role |
|---|---|
| `ArchipelagoWorkspace<A, I>` | Island tree + focus order for one archipelago |
| `ArchipelagoRouter<A>` | Exclusive active archipelago + enter/leave |
| `ArchipelagoNav<A>` | Archipelago navigation intents |
| `ChromeRoute<A, I>` | Archipelago **or** island chrome route |
| `ArchipelagoMessage` | Product msg → `ChromeRoute` |
| `route_archipelago_nav` / `route_chrome_message` | Apply chrome routes |
| `activate_archipelago_islands` | Re-handoff focus after archipelago change |

## 5. Frame slots (app)

Chrome does not hardcode TitleBar widgets. Per archipelago the app mounts:

```text
TitleBar presentation
Body                   (assemble IslandNode → island Entities)
StatusBar or none
```

## 6. Lifecycle

| Event | Chrome | App |
|---|---|---|
| Enter archipelago B | `router.enter(B)` | save island focus; remount frame B |
| Leave B | `router.leave()` | remount A; restore island focus |
| Island ClaimFocus | `ChromeRoute::Island` | only if A active |
| Overlay open | — | may save island focus independently |

## 7. Non-goals

- Product archipelago enums inside chrome
- Overlay stack priority inside `ArchipelagoRouter`
- Animations / reduced-motion
- Dynamic plugin archipelago marketplace
- Documentation of any specific product monorepo
