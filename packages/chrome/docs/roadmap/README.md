# Roadmap

Status and planning for this chrome kit.  
Feature contracts: [`../features/`](../features/).  
Design: [`../design/`](../design/).

## Acceptance rule

A backlog item is **done** only when **API + tests + design/feature docs** match
real consumer use (no speculative public surface).

## Priority

1. Archipelago **runtime** loop (workspace + route on the real app path)  
2. **ListKeyboard** as the only list cursor in consumers  
3. List/tree **composite** contracts  
4. Overlay **focus pipeline** in app host lifecycle  
5. Application-global **theme** + domain palette split

## Epic status

### A — Archipelago model closure

| ID | Item | Status |
|---|---|---|
| A1 | Semantics locked (body = islands) | done |
| A2 | Workspace drives layout + focus_order | done |
| A3 | Product path uses chrome route API | done |
| A4 | Secondary places use real islands | done |

### B — Island focus hardening

| ID | Item | Status |
|---|---|---|
| B1 | `try_focus` safe transfer | done |
| B2 | `assert_covers` dupes/extras | done |
| B3 | `FocusTransition { from, to }` | done |

### C — Router transitions

| ID | Item | Status |
|---|---|---|
| C1 | `Unchanged` / `Changed { from, to }` | done |
| C2 | Route API returns real transition | done |
| C3 | Optional `restore_kind` | todo (optional) |

### D — List / tree keyboard

| ID | Item | Status |
|---|---|---|
| D1 | `ListKeyboard` controller | done |
| D2 | Row `keyboard_focused` paint | done |
| D3 | Shared selectable list row primitive | done |
| D4 | TreeList composite contract | done |
| D5 | Consumers use ListKeyboard only | done |
| D6 | A11y semantic roles | blocked by GPUI 0.2 role API; keyboard parity done |

### E — Overlay composite

| ID | Item | Status |
|---|---|---|
| E1 | Responsive envelope | done |
| E2 | Scrollable body | done |
| E3 | `OverlayFocusSession` contract | done |
| E4 | Host wires session + restore | done |
| E5 | Consumers pass viewport | done |
| E6 | Tab focus trap | todo (optional) |

### F — Theme system

| ID | Item | Status |
|---|---|---|
| F1 | Application-global theme snapshot; per-window explicitly unsupported | done |
| F2 | Palette variants | done |
| F3 | Chrome vs domain role colors | done |
| F4 | Helpers read theme handle only | done |

## Suggested PR slices

| PR | Features | Goal |
|---|---|---|
| PR-1 | A2, A3 | Kill speculative Archipelago API |
| PR-2 | D5 | ListKeyboard is the only cursor in consumers |
| PR-3 | D3, D4 | Composite list/tree contracts |
| PR-4 | E4 | Overlay focus pipeline |
| PR-5 | F1–F4, B3, A1/A4/E5 | Theme + remaining partials |

## Snapshot

| Status | IDs |
|---|---|
| done | A1 A2 A3 A4 B1 B2 B3 C1 C2 D1 D2 D3 D4 D5 E1 E2 E3 E4 E5 F1 F2 F3 F4 |
| todo (optional) | C3 E6 |
| upstream-limited | D6 |
