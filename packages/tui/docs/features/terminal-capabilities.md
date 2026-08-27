# Terminal Capabilities

> Status: implemented (manual verification pending)
>
> Package: `piko-tui`

## Overview

Terminal Capabilities is the TUI-wide contract for adapting input, output, and
terminal-session behavior to the capabilities available in the current client
environment. It provides one runtime profile to keyboard routing, rendering,
text measurement, pointer input, paste handling, guidance, and terminal
cleanup. Product features consume semantic behavior and must not branch on a
terminal brand, `TERM` value, multiplexer name, or operating system.

The detected profile is local, ephemeral client state. It is not host session
state and is never persisted in the session journal.

## Problem

The TUI currently enables terminal modes directly in `TerminalGuard`, exposes
raw Crossterm events to feature-oriented input routing, and assumes one text
width policy. This gives a useful baseline but leaves compatibility behavior
distributed across unrelated modules:

- modified Enter is distinguishable only when enhanced keyboard reporting is
  available;
- bracketed paste, mouse capture, and alternate-screen cleanup are managed as
  one all-or-nothing sequence;
- keyboard hints can advertise a chord the current terminal cannot report;
- text editing and shared line layout can disagree about Unicode atomicity;
- initialization failure can occur after raw mode has already been enabled;
- compatibility cannot be verified through one deterministic runtime profile.

## User journeys

1. A user runs piko directly in a terminal with enhanced keyboard reporting.
   Modified keys retain their modifiers and the guidance row advertises the
   preferred newline chord.
2. A user runs piko through a baseline-capability terminal, SSH hop, tmux, or
   screen. The TUI uses conservative input semantics and advertises a reliable
   configured fallback rather than silently treating newline as submit.
3. A terminal supports only a reduced color level. Semantic UI states remain
   distinguishable through the effective palette and non-color cues.
4. A terminal cannot use an optional input mode. Keyboard interaction remains
   complete without mouse or bracketed-paste-specific behavior.
5. Startup fails after one or more terminal modes were changed. Every applied
   mode is restored before the error reaches the shell.
6. A user resizes a narrow terminal or edits CJK and composed Unicode text.
   cursor placement, wrapping, hit testing, and growth use the same terminal
   column model.

## Behavior contract

### Capability resolution

- One immutable effective runtime profile is resolved before normal event
  processing begins.
- Active probes are preferred when the backend provides a bounded probe.
- Environment values such as `TERM`, `COLORTERM`, multiplexer markers, and SSH
  markers are advisory evidence only.
- Unknown capability support resolves to a conservative behavior.
- Terminal names never select product behavior directly.
- The effective profile can be included in diagnostics, but not in durable
  host session state.

### Keyboard

- Raw terminal key events are normalized before scoped command resolution or
  focus routing.
- Feature reducers receive semantic actions and do not inspect terminal
  protocols.
- `Enter` remains submit.
- Enhanced modified Enter may provide the preferred newline chord.
- `Ctrl+J` is selected as the fallback newline chord when modified Enter
 cannot be reported reliably; enhanced mode selects `Shift+Enter` instead.
- Guidance lists only chords that the effective binding registry marks active
  and the terminal profile can distinguish.
- Missing enhanced keyboard reporting must not disable ordinary typing,
  navigation, submit, cancel, or exit.

### Pointer and paste

- Mouse capture and bracketed paste are optional terminal session modes.
- Failure or absence of mouse capture leaves all product actions reachable by
  keyboard.
- Paste is normalized as one semantic event when bracketed paste is available.
  Plain key input remains valid when it is not.
- Paste continues to respect the active focus owner after normalization.

### Color and presentation

- Rendering consumes an effective color level rather than inferring support in
  individual components.
- Critical state is never communicated by color alone.
- Reduced color support maps semantic theme slots to the effective palette;
  it does not change feature structure or state.
- Terminal-default colors remain a valid conservative fallback.

### Text width and Unicode

- Cursor movement and deletion operate on grapheme clusters, except that
  structured editor references remain higher-level atomic blocks.
- Wrapping, truncation, cursor placement, and pointer-to-caret mapping share
  one column-width policy compatible with the Ratatui backend.
- Hard newlines and soft wraps both contribute to composer auto-resize.
- A grapheme is never split merely to fit a one-column remainder.
- When the terminal and backend cannot agree on an Ambiguous-width glyph, the
  TUI must fail safely through clipping rather than invalid buffer geometry.

### Terminal lifecycle

- Terminal mode activation is transactional.
- The runtime records each mode after it is successfully enabled.
- Normal exit, panic cleanup, and partial initialization failure restore only
  the modes that were applied, in reverse order.
- Cleanup is idempotent.
- Capability probing has a bounded duration and cannot leave the process in raw
  mode after failure.

## Architecture boundary

- `piko-tui` owns detection, terminal session modes, event normalization, text
  policy selection, and product fallbacks.
- `piko-tui-layout` remains a terminal-independent geometry and focus library.
- Composer, pointer, theme, and other features consume narrow semantic views of
  the effective profile; none owns capability discovery.
- hostd remains authoritative for persisted `[tui]` preferences if terminal
  overrides are introduced later. Detected facts remain local to the client.
- No protocol changes are required for the initial capability foundation.

## Configuration

The initial slice adds no persisted terminal-mode settings. It uses
conservative built-in terminal policy. Keybinding customization follows the
host-owned rule model in [Keybindings and Command Routing](./keybindings.md).
A later feature may add host-owned `[tui.terminal]` preferences, but it must
define a pre-enter settings handshake rather than reading host settings
directly from the client.

## Acceptance criteria

- [x] One effective terminal profile is created outside `AppState` domain data
     and is available to input, render, guidance, and session lifecycle code.
- [x] No product feature branches on terminal brand, multiplexer brand, or OS.
- [x] Enhanced and baseline keyboard profiles both provide distinct submit and
     newline paths.
- [x] The initial enhanced profile requests disambiguation without replacing
     text-producing keys whose associated text Crossterm cannot preserve.
- [x] Input normalization is covered by deterministic capability-matrix tests.
- [x] Terminal initialization rolls back every successfully applied mode after
     failure at any later activation step.
- [x] Normal exit, panic cleanup, and `Drop` cleanup are idempotent.
- [x] Mouse-unavailable and bracketed-paste-unavailable profiles retain a
     complete keyboard workflow.
- [x] Theme resolution has explicit truecolor, ANSI-256, ANSI-16, and terminal-
     default behavior.
- [x] Editor and shared line layout use grapheme-safe column operations.
- [x] Hard-newline and soft-wrap composer growth remains covered at narrow and
     wide terminal sizes.
- [x] A PTY test covers enhanced input, baseline fallback, paste, resize, and
     cleanup sequences.
- [ ] Manual smoke evidence covers direct terminal, tmux, and SSH paths on the
     supported development platforms.
- [x] `cargo test -p piko-tui` and workspace clippy pass.

## Non-goals

- Emulating terminal-specific behavior by product feature.
- Persisting detected capabilities in host sessions.
- Adding inline terminal image protocols.
- Pixel-precise mouse or touch input unavailable through Crossterm.
- Claiming screen-reader accessibility from terminal capability detection.
- Solving every Unicode Ambiguous-width disagreement independently of Ratatui.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Ownership | TUI runtime foundation | Capabilities affect every input and paint surface |
| Terminal identification | Capability-based, never brand-based | Multiplexers and remote hops invalidate brand assumptions |
| Durable authority | Detected facts stay client-local | They describe this process environment, not the session |
| Unknown support | Conservative fallback | Optional enhancement must not become required functionality |
| Layout crate impact | None | Geometry solving is independent from terminal I/O |
| Initial configuration | No new persisted settings | Avoid a client-side settings authority and startup race |
| Fallback newline | `Ctrl+J` | Crossterm 0.29 preserves byte `0x0a` as `Ctrl+J` in raw mode |
