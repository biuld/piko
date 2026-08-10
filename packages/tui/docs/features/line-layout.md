# Line layout primitives

> Status: reviewed
>
> Product layout *principles* for stream chrome: [ui-ux.md](./ui-ux.md)
> (*Stream projection layout principles*). This doc is the **shared toolkit
> contract**, not “how timestamps” or “how tool titles” are designed.

## Overview

Line layout is a small **text-column** toolkit for in-band rows that need left
content and an optional right affix without flex region geometry.
Implementation: `packages/tui/src/ui/line_layout.rs`.

It is **not** `piko-tui-layout` (shell / plane / modal flex).

## Contract

- Measure columns with **`unicode-width` only**.
- Soft-wrap and truncation are column-based; hard newlines remain row breaks.
- Multi-line bodies that reserve a right zone share one left-column width for
  every row; the affix paints on the first row only; continuations leave the
  right zone blank.
- Optional matching outer edge inset on the right when the row pads left.

Concrete spacer/edge constants and call sites live in code. Who consumes the
API (messages, tool titles, …) is an implementation choice, not a product
layout PRD per consumer.

## Non-goals

- Not a catalog of per-projection layouts.
- Not Selectable `Table` / multi-row panel layout.
- Not flex region solving or focus stacks.
- Not terminal-capability discovery of Ambiguous width.
