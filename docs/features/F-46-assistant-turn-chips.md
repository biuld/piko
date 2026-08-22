# F-46: Assistant turn activity chips

> Status: design
> Priority: P0
> Source evidence: product direction — chat-reaction visual model (tapback capsules + quoted reply), user decision 2026-08-23
> Design: [D-63](../design/D-63-assistant-turn-chips.md)
> Amends: F-45 Timeline conversation blocks — supersedes the independent thinking/tool card contract

## Summary

One assistant turn renders as **one bubble** whose content is a chronological
flow of *activity chips* and *response text segments*. Thinking and tool calls
are no longer collapsible cards; they are compact capsule chips laid out in
time order between text blocks. A chip's icon doubles as its status: a spinner
while that activity streams, an alert icon when a tool fails. Clicking a chip
opens a detail overlay (full thinking text / structured tool body) without
disturbing the timeline flow.

## Problem

Real agent turns interleave `think → toolcall → think → toolcall → response →
think`. F-45's independent cards break this rhythm into heavy stacked cards:
thinking text dominates the canvas, tool cards repeat their own chrome per
call, and the visual order of the turn is lost.

## User journeys

1. A turn starts; before any text, a row of chips hugs the bubble's top-left:
   `Thinking` (spinner) then `Bash $ rg …` (spinner). Text has not arrived.
2. Response text arrives: chips wrap onto their own line, the markdown body
   starts on the next line.
3. After text, the agent thinks again and calls another tool: a new chip row
   starts below the finished text block, in time order.
4. A tool fails mid-run: its chip switches to the failure icon with danger
   tint; the rest of the turn keeps flowing.
5. Clicking any chip opens the detail overlay (Escape / backdrop closes);
   the timeline underneath does not reflow.
6. Copy/Quote on the bubble uses the response text segments as plaintext.

## In scope

- Island `ActivityChip` primitive (capsule: leading status icon/spinner,
  truncated label, fixed-width variant, status tint) and chip-row container.
- Desktop turn-flow mapping: consecutive assistant-side timeline items group
  into one row preserving segment order (`Thinking | Tool | Text`).
- Chip detail overlay via the existing temporary-layer host (backdrop +
  Escape + focus restore).
- Streaming states: spinner on the active thinking/tool chip; caret on the
  streaming text tail (unchanged).

## Out of scope

- hostd / orchd / protocol changes. Anchored near-cursor popovers (the
  centered overlay ships first). Persisted chip state. Syntax highlighting.
- User bubbles, system entries, composer, tabs (F-44 rules stand).

## Acceptance criteria

- [ ] Consecutive `RealtimeDraft | Committed(Assistant) | Tool` items render
      as one assistant bubble; a user message or session entry ends the run.
- [ ] Chips appear in chronological order; each maximal chip run occupies its
      own wrapped row; every text segment starts on a new line below the run
      that preceded it.
- [ ] Thinking chips are fixed width; tool chips hug their label and clamp.
- [ ] While streaming, the active thinking/tool chip shows an indeterminate
      spinner; completed tools show a neutral icon; failed tools show the
      danger icon; cancelled shows the stopped icon.
- [ ] Clicking a chip opens the detail overlay; Escape and backdrop dismiss;
      focus returns to the timeline; the list does not remeasure.
- [ ] Bubble selection/plaintext covers text segments; Quote inserts them.
- [ ] Scroll stays virtualized: offsets count groups, not payload rows; a
      paint maps only the visible turn.

## Product decisions

- Chips over quote-blocks for thinking (user decision): keeps the whole turn
  scannable at chip height; detail moves to overlay instead of inline expand.
- Overlay instead of accordion/popover: reuses the existing temporary-layer
  contract; anchored popovers are a later island primitive.
- One bubble per turn even when it contains only chips (activity-only turns
  still read as part of the conversation).

## Open questions

- Determinate progress for long-running exec tools (needs host signal).
