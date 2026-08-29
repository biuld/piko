# Dock Stack (plane coexistence)

> Status: draft
> Design: [dock-coexistence.md](../design/dock-coexistence.md)
> Kind: standalone TUI infrastructure feature

## Overview

Dock Stack owns the vertical budget for plane bands above the composer. It
keeps the Stream readable while optional completion content coexists with the
resident Guidance and Composer anchors.

Todo is deliberately outside this stack. The current todo list is opened with
`/todo` as a centered overlay; see [todo-list.md](./todo-list.md).

## Catalog

| Order | Band | Residency | Role |
|---:|---|---|---|
| 0 | Boundary | anchor | one reserved blank row between Stream and dock content |
| 1 | Suggest | ephemeral | slash command or `@` completion rows |
| 2 | Guidance | anchor | notice or contextual key hints |
| 3 | Composer | anchor | editor |

The Boundary always keeps a one-row layout grant. It **must not paint a top
border, horizontal rule, provider title, or candidate count**. It stays blank
whether Suggest is active or idle.

## Offer and grant

Providers submit active/preferred/minimum heights. The stack grants heights in
catalog order under a Stream floor. Inactive ephemeral bands receive zero.
Providers paint only inside their grant.

Shrink order is:

1. Suggest toward its minimum.
2. Composer toward its editor minimum.
3. Protected Boundary and Guidance only in pathological frames.

## Acceptance

- [ ] Idle plane order is Stream, blank Boundary, Guidance, Composer.
- [ ] Boundary occupies exactly one row and paints no horizontal rule.
- [ ] Suggest does not place provider metadata in Boundary.
- [ ] Todo state never creates a Dock Stack grant or plane region.
- [ ] Stream retains its configured floor under pressure.
