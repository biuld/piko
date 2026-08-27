# ADR-023: Use normalized TUI input with host-owned scoped bindings

> Status: accepted
> Date: 2026-08-27

## Context

The TUI previously combined raw Crossterm matching, a global keymap, feature
specific branches, and configuration paths that could silently disagree. Its
terminal modes were also activated as one loosely tracked sequence, so a
failure after raw mode or alternate-screen activation could leave the shell
in a partially modified state.

The terminal can also report different levels of keyboard detail. In
particular, the current runtime cannot rely on distinguishing `Shift+Enter`
on every path, while it can preserve ordinary text and `Ctrl+J` in the
baseline profile.

## Decision

- The TUI creates one process-local `TerminalProfile` before normal event
  processing. The profile is the sole input, rendering, guidance, text-width,
  and terminal-session capability context; detected facts are not persisted.
- Crossterm events cross one adapter boundary into `NormalizedInput`. Product
  routing consumes semantic key strokes, phases, text, paste, pointer, resize,
  and focus events rather than matching raw terminal types in feature code.
- Commands are declared once in a stable catalog. A scoped binding registry
  compiles built-in rules and host-projected overrides, validates reachability
  and conflicts, and dispatches through the existing root `Action` boundary.
  Explicit aliases for one command are valid for input; guidance selects one
  deterministic canonical key.
- hostd owns the persisted `[tui.keybindings]` namespace and recursively
  merges global and project objects by rule ID. The TUI does not read, detect,
  migrate, or otherwise consult the old `keybindings.json` paths. This is a
  deliberate clean break and removes the possibility of two live authorities.
- The default newline rules are capability-dependent: enhanced keyboard paths
  use `Shift+Enter`, while the baseline fallback uses `Ctrl+J`; plain `Enter`
  remains submit.
- Terminal modes are enabled transactionally. Each successful mode is journaled
  and cleanup attempts every applied mode in reverse order. Normal exit,
  initialization rollback, panic cleanup, and `Drop` share idempotent cleanup
  semantics. Optional mouse, paste, and keyboard enhancements cannot make the
  keyboard workflow unavailable.
- Semantic colors are quantized once from the effective color level, and all
  editor/layout column operations use the centralized grapheme-safe text
  policy.

## Consequences

- Key handling, hints, diagnostics, and configuration now use one command and
  binding vocabulary, with terminal reachability applied consistently.
- A terminal that cannot report an enhanced chord still has a complete
  baseline workflow, and the user-facing terminology does not imply support
  for an old configuration format.
- Invalid host binding updates can be rejected while the previous valid
  registry remains active; hostd does not need to understand command semantics.
- The runtime has more explicit seams and tests; the process-level PTY suite
  is implemented, while manual cross-terminal smoke evidence remains required
  verification work.
- Old JSON keybinding files require deliberate removal or replacement by the
  user; they are not automatically migrated.
