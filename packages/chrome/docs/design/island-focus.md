# Feature: Island focus table hardening

> IDs: **B1–B3**  
> Layer: L2  
> Parent: [roadmap](../roadmap/README.md) · [island-runtime feature](../features/island-runtime.md) · [island-interaction.md](island-interaction.md)

## Problem

Focus handoff that updates the ring before verifying a registered slot can
leave the ring pointing at a non-existent island and clear all focus chrome.

## Requirements

### B1 — Safe transfer (done)

- `IslandFocusTable::try_focus` validates slot **before** mutating ring.
- On unknown id: ring unchanged, paint unchanged, return `Err(UnknownIsland)`.
- `focus` is a convenience that `debug_assert`s on failure.

### B2 — Registration integrity (done)

- `assert_covers(expected)`:
  - rejects **duplicate** ids in `expected`;
  - rejects **missing** slots;
  - rejects **extra** registered slots not in `expected`.

### B3 — Focus transition payload (done)

- On success, `try_focus` / `route_focus_message` / `claim_focus_id` return
  `FocusTransition { from, to }`.
- `FocusRing::transfer` is the pure ownership move used by claim.
- Enables logging, tests, and “already focused” policies without re-reading the
  ring (`FocusTransition::unchanged`).

## Non-goals

- Product-specific focus order (app provides `focus_order`).
- In-list row keyboard (Epic D / `ListKeyboard`).

## Acceptance tests

- Unit: `try_focus` unknown id leaves prior `ring.focused()` intact
  (`claim_focus_id` path).
- Unit: `assert_covers` fails on duplicate expected and on extra slots.
- Unit (B3): `FocusRing::transfer` reports `from`/`to`; unchanged when same id.
