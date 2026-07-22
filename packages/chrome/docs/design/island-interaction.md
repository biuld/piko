# Design: Island interaction

> Status: normative implementation design for this chrome kit  
> Crate: [`AGENTS.md`](../../AGENTS.md)  
> Source: [`src/island/contract.rs`](../../src/island/contract.rs)  
> Feature: [island-runtime](../features/island-runtime.md)

## 1. Purpose

How islands isolate, focus, and message the host. Archipelagos are defined in
[archipelago.md](archipelago.md).

## 2. Layering

```text
┌─────────────────────────────────────────────────────────────┐
│ App                                                          │
│  product IslandId · domain msgs · layout prefs               │
│  impl IslandView / IslandHost / IslandMessage                │
├─────────────────────────────────────────────────────────────┤
│ Chrome kit                                                   │
│  IslandPanel · FocusRing · FocusTable · FocusMsg · defer     │
│  route_focus_message · schedule_island_message               │
└─────────────────────────────────────────────────────────────┘
```

## 3. Isolation

| Allowed | Forbidden |
|---|---|
| Local UI, `apply` VMs, emit msgs | Sibling Entity mutation |
| Own FocusHandle / Input | Broadcast bus rebuilds |

## 4. Directed messaging

```text
Island → schedule_island_message → IslandHost
  ├─ as_focus_msg → try_focus / route_focus_message
  └─ product domain arms
```

Always deferred (GPUI reentrancy).

## 5. Focus: two channels

| Channel | Owner |
|---|---|
| Chrome ring | `FocusRing` + table paint |
| Keyboard caret | `IslandView::take_keyboard_focus` (Activate vs Claimed) |

Unknown focus ids: `try_focus` fails without mutating the ring.

## 6. Three graphs (do not collapse)

1. **Layout** — `IslandNode` + prune (chrome tree + app policy).  
2. **Selection** — domain state (app only).  
3. **Interaction** — focus msgs + product msgs.

## 7. Non-goals

- Product leaf ids or domain enums in chrome.
- Session kernels, broadcast buses, plugin marketplaces.
