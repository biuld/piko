# Design: Terminal Capability Runtime

> Status: implemented (manual verification pending)
>
> PRD: [../features/terminal-capabilities.md](../features/terminal-capabilities.md)

## Goal

Create one TUI runtime boundary that discovers terminal facts, selects safe
product behavior, owns terminal mode activation and cleanup, and normalizes raw
events before feature routing.

## Ownership

The implementation lives under `packages/tui/src/terminal/`:

```text
terminal/
├── mod.rs            public runtime facade
├── capability.rs     detected facts and detector
├── policy.rs         conservative product policy
├── profile.rs        resolved effective behavior
├── session.rs        mode activation and transactional cleanup
├── input.rs          Crossterm event normalization
└── text.rs           backend-compatible grapheme/column policy
```

The current `src/tui.rs` responsibility moves into `terminal/session.rs`.
`piko-tui-layout` does not change and does not depend on Crossterm.

## Model

Capability facts, requested terminal modes, and product behavior are distinct.
An escape sequence that requests mouse reporting is not proof that the terminal
supports it, and a supported feature need not be enabled by product policy.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct KeyboardEnhancements: u8 {
        const DISAMBIGUATE = 0b00001;
        const EVENT_TYPES = 0b00010;
        const ALTERNATE_KEYS = 0b00100;
        const ALL_KEYS = 0b01000;
        const ASSOCIATED_TEXT = 0b10000;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyboardCapabilities {
    pub progressive_protocol: Support,
    pub reported_flags: KeyboardEnhancements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorLevel {
    TerminalDefault,
    Ansi16,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalCapabilities {
    pub keyboard: KeyboardCapabilities,
    pub color: ColorLevel,
    pub mouse: Support,
    pub bracketed_paste: Support,
    pub focus_events: Support,
    pub synchronized_output: Support,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalModePlan {
    pub keyboard_flags: KeyboardEnhancements,
    pub mouse_capture: bool,
    pub bracketed_paste: bool,
    pub focus_events: bool,
    pub synchronized_output: bool,
    pub alternate_screen: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalProfile {
    pub capabilities: TerminalCapabilities,
    pub modes: TerminalModePlan,
    pub color: ColorLevel,
    pub active_keyboard_flags: KeyboardEnhancements,
    pub key_reachability: KeyReachability,
    pub text: TerminalTextPolicy,
}
```

`TerminalCapabilities` contains observed or conservatively inferred facts.
Keyboard support is flag-based rather than a baseline/enhanced boolean because
the protocol is progressively enhanced and implementations may expose a
subset. `TerminalModePlan` contains what the session will request.
`TerminalProfile` records the flags actually active after negotiation and is
the immutable effective contract consumers receive.

Environment evidence is kept private to the detector. `TERM=xterm-kitty`,
`TMUX`, or an SSH marker may influence a fact, but no downstream type exposes a
terminal brand.

## Runtime flow

```text
process environment + bounded backend probes
                    |
                    v
          TerminalCapabilities
                    +
        built-in conservative policy
                    |
                    v
             TerminalProfile
                    |
          TerminalSession::enter
                    |
        +-----------+------------+
        |           |            |
        v           v            v
 InputNormalizer  RenderContext  GuidanceContext
        |           |            |
        +-----------+------------+
                    v
             existing features
```

Detection occurs before raw mode where possible. A probe that requires raw
mode is executed through the transactional session state and has a fixed
timeout. Normal event processing starts only after the profile and session are
ready.

The profile belongs to a `TuiRuntime` object, not to durable product state:

```rust
pub struct TuiRuntime {
    pub session: TerminalSession,
    pub profile: TerminalProfile,
    pub input: InputNormalizer,
}
```

`AppState` continues to contain user-visible TUI state. Code that needs runtime
facts receives a narrow context parameter rather than reaching through
`AppState`.

## Capability detection

`CapabilityDetector` is a trait so the matrix can be tested without a real
terminal:

```rust
pub trait CapabilityDetector {
    fn detect(&self) -> TerminalCapabilities;
}
```

The production detector:

1. uses Crossterm's bounded progressive-enhancement query and records the
   reported flags;
2. derives color level from standard terminal environment evidence;
3. treats non-queryable optional modes as `Unknown`;
4. records diagnostic reasons internally for `/diagnostics` without exposing
   terminal names as behavior switches;
5. resolves malformed, missing, or contradictory evidence conservatively.

The initial policy may request widely tolerated bracketed-paste and mouse modes
when support is `Supported` or `Unknown`, while still ensuring the application
works if no corresponding events arrive. Unsupported never becomes enabled.

## Transactional terminal session

`TerminalSession` records applied modes individually:

```rust
struct AppliedModes {
    raw: bool,
    alternate_screen: bool,
    keyboard_flags_pushed: bool,
    bracketed_paste: bool,
    mouse_capture: bool,
    focus_events: bool,
}
```

Each successful activation flips its flag immediately. An error invokes one
idempotent `restore()` method, which disables modes in reverse activation order
and shows the cursor. `exit`, `Drop`, and the panic hook all delegate to the
same restoration primitive. Cleanup continues after an individual disable
failure and returns the first error after attempting the complete rollback.

The panic hook must not duplicate an incomplete subset of escape sequences.
It calls an emergency restoration function shared with `TerminalSession` and
then invokes the previous hook.

### Keyboard enhancement selection

The initial runtime requests only
`KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES`. It does not request
`REPORT_ALL_KEYS_AS_ESCAPE_CODES`: the Kitty protocol separates key identity
from associated text, and Crossterm 0.29 does not expose the complete associated
text enhancement. Preserving keyboard-layout and IME-produced text has priority
over optional shortcuts.

`REPORT_EVENT_TYPES` remains disabled until a product feature requires release
events. Repeated baseline press events are classified by the command catalog's
repeat policy after normalization.

## Input normalization

`terminal::input` converts Crossterm types into terminal-neutral events:

```rust
pub enum NormalizedInput {
    Key {
        stroke: KeyStroke,
        phase: KeyPhase,
        text: Option<String>,
    },
    Paste(String),
    Pointer(PointerEvent),
    Resize { width: u16, height: u16 },
    FocusGained,
    FocusLost,
}

pub enum KeyPhase {
    Press,
    Repeat,
    Release,
}
```

The scoped binding resolver consumes `NormalizedInput`, an immutable binding
context, and the effective registry. Focus adapters never own terminal protocol
distinctions.

Newline reachability follows the effective profile; actual command bindings
belong to the keybinding registry:

- enhanced profile marks `Shift+Enter` reachable and selects its newline rule;
- baseline profile selects the `Ctrl+J` newline rule;
- the rules are mutually exclusive even though `Ctrl+J` may be physically
  reportable in an enhanced profile;
- `Enter` is never reinterpreted as newline;
- if the backend cannot distinguish a configured chord, guidance does not
  advertise it.

Crossterm 0.29's Unix parser intentionally preserves byte `0x0a` as
`Ctrl+J` while raw mode is active, distinct from carriage-return `Enter`.
The PTY harness verifies that contract on supported runtime paths. If another
backend cannot preserve it, that backend must supply a tested distinct chord
rather than guessing from terminal identity.

## Render and guidance contexts

Rendering receives a small immutable context:

```rust
pub struct RenderContext<'a> {
    pub terminal: &'a TerminalProfile,
    pub bindings: &'a BindingRegistry,
}
```

This context is passed at the root paint boundary. Components receive only the
derived values they need, such as effective theme colors, text metrics, or an
already formatted key hint. This prevents arbitrary capability checks from
spreading through feature modules.

Theme resolution quantizes semantic colors once for the effective color level.
Components continue to use semantic theme slots and never perform color-level
checks themselves.

## Text policy

`TerminalTextPolicy` centralizes grapheme iteration and terminal-column
measurement. The initial policy remains aligned with Ratatui and
`unicode-width`; it fixes inconsistent call sites but does not pretend it can
override Ratatui's buffer-width model.

Migration order:

1. expose grapheme-safe `width`, `prefix`, `truncate`, and `wrap` operations;
2. migrate `ui::line_layout` and `ui::line_wrap`;
3. migrate editor movement, deletion, visual lines, caret math, and pointer
   placement;
4. migrate remaining direct `UnicodeWidthStr` consumers;
5. add an architecture test preventing new direct width calculation outside
   `terminal::text` and its adapter boundary.

Structured editor references stay atomic even when their display label is
wider than the available row. A too-wide grapheme or reference occupies a row
and is clipped by paint; it is never split into invalid text.

## Settings and host authority

The first implementation uses fixed conservative policy and adds no settings.
Detected facts are never sent to hostd as authoritative state.

If overrides are later introduced, they belong under host-owned
`[tui.terminal]`. Because terminal modes are entered before the current async
configuration bootstrap completes, that later feature must add an explicit
pre-enter settings handshake or mark affected settings as restart-required.
The TUI must not read `settings.toml` directly.

## Diagnostics

Diagnostics may show:

- effective keyboard protocol;
- effective color level;
- requested terminal modes;
- fallback reasons and probe errors;
- whether the process is under a multiplexer or remote transport as
  informational evidence.

Diagnostics must not expose environment values that may contain secrets and
must not make host-visible session state depend on this information.

## Testing

### Pure matrix tests

- supported, unsupported, and unknown optional capability resolution;
- enhanced versus baseline newline chords;
- color-level palette selection;
- contradictory and missing environment evidence;
- normalized key, paste, pointer, focus, and resize events.

### Lifecycle fault injection

A fake terminal writer fails after each activation step. Every case asserts
that earlier modes are disabled in reverse order, the cursor is shown, and a
second cleanup is a no-op.

### Rendering tests

Ratatui `TestBackend` covers narrow/wide frames, resize, CJK, combining marks,
ZWJ emoji, structured references, composer growth, clipping, and cursor rows.

### PTY tests

A process-level harness drives the binary through a pseudo-terminal and checks:

- enhanced keyboard response and baseline timeout;
- distinct submit and newline input;
- bracketed paste boundaries;
- resize delivery and repaint;
- normal and interrupted cleanup sequences.

`cargo test -p piko-tui --test terminal_pty -- --test-threads=1` runs these
checks with a deterministic mock hostd. The fixtures do not require a
developer's interactive terminal or persisted piko home.

### Manual verification matrix

Record smoke evidence for at least:

- Terminal.app or iTerm2;
- Kitty, WezTerm, or Ghostty with enhanced keyboard reporting;
- tmux;
- an SSH path;
- Windows Terminal when Windows is an actively supported build target.

## Migration plan

1. Introduce capability, policy, profile, and detector types with matrix tests.
2. Replace `TerminalGuard` with transactional `TerminalSession` and fault tests.
3. Add normalized terminal input and adapt the event loop boundary.
4. Integrate the scoped binding registry, verified baseline newline fallback,
   and capability-aware guidance.
5. Resolve theme colors once for the effective color level.
6. Centralize grapheme/column operations and migrate editor plus shared layout.
7. Add PTY coverage and record the manual verification matrix.
8. Update the PRD status and implementation checkboxes after behavior lands.

## Package impact

| Package | Change |
|---|---|
| `piko-tui` | New runtime foundation and consumer migrations |
| `piko-tui-layout` | None |
| `piko-hostd` | None in the initial slice |
| `piko-protocol` | None |
| `piko-comms` | None unless PTY fixtures reuse a declared bridge |

## Risks

- Terminal probing may block or consume user input. Every active probe must be
  bounded and parser-owned.
- Environment evidence can be wrong through multiplexers or SSH. It remains
  advisory and resolves conservatively.
- Custom width rules can diverge from Ratatui. The initial policy stays backend
  compatible and centralizes behavior before adding overrides.
- Passing the full profile everywhere would create a new global dependency.
  Consumers receive narrow derived contexts instead.
- Terminal cleanup during panic cannot guarantee recovery after process-wide
  abort or kill signals. Shell-level `reset` remains outside application
  guarantees.

## Open questions

1. Which color detector should be used without creating conflicting ANSI logic
   beside Crossterm and Ratatui?
2. Should focus events and synchronized output land in the initial mode plan or
   remain modeled-but-disabled until a concrete consumer exists?
3. What platforms constitute piko's supported TUI release matrix today?
