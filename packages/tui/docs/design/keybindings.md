# Design: Scoped Command and Keybinding Runtime

> Status: implemented
>
> PRD: [../features/keybindings.md](../features/keybindings.md)
> Related: [terminal-capabilities.md](terminal-capabilities.md)

## Goal

Replace the parallel `KeyAction`, string-ID, raw-key, and feature-special-case
paths with one deterministic pipeline from normalized terminal input to a
catalogued command and then to the existing root `Action` reducer boundary.

## Review findings

### R1 — The current keymap is global, while behavior is contextual

`Keymap` stores `HashMap<serialized chord, KeyAction>`. `Up`, `Down`, `Enter`,
`Esc`, `Ctrl+P`, and `Ctrl+E` are assigned one global action and later
reinterpreted by `InputRouter`. Context therefore lives outside the keymap that
claims to define the binding.

### R2 — Raw keys bypass configuration

The focus router contains dozens of direct `KeyCode` and modifier branches for
approval, tool interaction, SummaryPrompt, Tree, Sessions, Notifications, and
text filters. A user can change a `KeyAction` binding while the hard-coded key
continues to execute.

### R3 — Command identity is duplicated and inconsistent

`KeyAction`, `action_from_id`, root `Action` variants, feature actions, and docs
all describe overlapping operations. Some configuration-only variants have no
default or router path, and aliases such as session resume map through different
intermediate variants.

### R4 — Overrides are not rule overrides

The current JSON object maps command ID to one chord, then inserts the chord
into a map. It cannot express multiple scoped rules, cannot explicitly unbind a
default, leaves a command's old default active, and silently overwrites chord
collisions.

### R5 — Precedence is procedural

`P1`, `P1.5`, focus-owner branches, suggestion interception, and editor
fallback collectively define precedence. Adding a surface requires editing the
central router, and blocking authority depends on branch placement.

### R6 — Terminal reachability is absent

The registry cannot distinguish a configured chord from a chord the terminal
path can actually report. Hints and configuration therefore treat
`Shift+Enter` on a baseline-capability terminal as equivalent to `Ctrl+J` even
though only the latter is reliable.

### R7 — Input event policy is implicit

The event loop discards release events but otherwise lets repeat events reach
all actions. The command registry cannot say that cursor movement repeats while
quit, delete-session, or approval submission must be press-only.

### R8 — Configuration failures are silent

Unreadable files, malformed JSON, unknown command IDs, invalid chords, and
collisions are ignored. Users cannot inspect what key the terminal reported or
why a command did not run.

## Reference practices

The design distills, rather than copies, these current primary-source models:

- The [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
  uses opt-in progressive enhancement, canonical modifier reporting, explicit
  capability queries, and stack-like push/pop restoration.
- [Crossterm 0.29 keyboard events](https://docs.rs/crossterm/0.29.0/crossterm/event/struct.KeyEvent.html)
  distinguish press/repeat/release only when the corresponding enhancement is
  active on Unix; its associated-text support is incomplete.
- [VS Code keybindings](https://code.visualstudio.com/docs/configure/keybindings)
  use stable command IDs, conditional activation, conflict inspection, and
  dispatch tracing.
- [Zellij keybindings](https://zellij.dev/documentation/keybindings) are
  explicitly divided into modes; its
  [non-colliding preset](https://zellij.dev/tutorials/colliding-keybindings/)
  demonstrates that scope and discoverable mode authority are preferable to a
  large set of always-global shortcuts.
- [Helix remapping](https://docs.helix-editor.com/remapping.html) separates
  named commands from per-mode keymaps. Piko adopts that separation but uses a
  deterministic canonical guidance while preserving explicit input aliases for
  one command.

Piko keeps its existing focus-stack product model. It does not adopt a modal
editor or a general expression engine merely because a reference has one.

## Target architecture

```text
Crossterm Event
      |
      v
terminal::InputNormalizer  + TerminalProfile
      |
      v
NormalizedInput::Key { stroke, phase, text }
      |
      +----> BindingContext snapshot + active ScopeStack
      |                         |
      v                         v
             BindingResolver
      key + scope + conditions + reachability
      |             |                 |
      |             |                 +--> diagnostic trace
      |             +--> no command -> declared TextSink
      v
CommandInvocation { command, source_rule }
      |
      v
CommandDispatcher / focused component adapter
      |
      v
root Action → AppState::dispatch
```

The reducer boundary does not change. This design replaces only how terminal
input selects a root action.

## Module layout

```text
input/
├── mod.rs
├── command/
│   ├── mod.rs          CommandId and invocation
│   ├── catalog.rs      metadata and allowed scopes
│   └── dispatch.rs     command → focused adapter → Action
├── binding/
│   ├── mod.rs
│   ├── chord.rs        canonical KeyStroke parser/formatter
│   ├── context.rs      typed context snapshot and condition atoms
│   ├── defaults.rs     built-in rules with stable IDs
│   ├── config.rs       host settings DTO compiler
│   ├── registry.rs     immutable compiled rules
│   ├── resolver.rs     scope-stack resolution
│   └── diagnostics.rs  validation and per-event trace
└── target.rs           focus-owned command/text adapter
```

Terminal-specific normalization remains in `terminal/input.rs`. `input/`
contains product commands, scope, and focus semantics.

## Normalized input

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

pub struct KeyStroke {
    pub code: Key,
    pub modifiers: Modifiers,
}

pub enum KeyPhase {
    Press,
    Repeat,
    Release,
}
```

`KeyStroke` has one canonical serialization. Modifier ordering, `return` versus
`enter`, `escape` versus `esc`, `BackTab`, letter case, keypad state, and
backend-specific aliases are normalized once.

Bindings are logical-key bindings. Crossterm does not currently provide a
portable physical scan-code contract comparable to desktop UI frameworks, so
the design does not promise keyboard-layout-independent physical bindings.

`text` is data produced by the key event, when the backend can preserve it. A
plain character fallback is derived only after command resolution and only for
the declared text sink.

## Terminal enhancement policy

Piko requests the minimum keyboard enhancement required for unambiguous
shortcuts. It does not request `REPORT_ALL_KEYS_AS_ESCAPE_CODES` merely to make
the keymap richer: the Kitty protocol makes associated text a separate
enhancement, while Crossterm 0.29 does not fully expose associated text. Losing
layout/IME-produced text is worse than losing an optional shortcut.

Initial policy:

- request `DISAMBIGUATE_ESCAPE_CODES` when supported;
- do not request release events until a consumer needs them;
- treat repeat as press for catalog commands marked repeatable;
- ignore repeat for commands marked press-only;
- select `Ctrl+J` for newline in the baseline profile;
- select `Shift+Enter` instead when the effective profile marks it reachable;
- never activate both newline rules in one context.

The Terminal Capabilities PTY suite is the authority for reachability. `TERM`
or terminal brand alone never enables a rule.

## Command catalog

```rust
pub struct CommandSpec {
    pub id: CommandId,
    pub title: &'static str,
    pub category: CommandCategory,
    pub scopes: &'static [ScopeKind],
    pub repeat: RepeatPolicy,
    pub enablement: fn(&BindingContext) -> bool,
    pub terminal_requirement: Option<TerminalRequirement>,
}
```

The catalog is the source for configuration validation, default rules,
guidance, diagnostics, and future command discovery. Dispatch is exhaustive:
adding a catalog command without an executable adapter fails a catalog test.

Commands fall into two families:

1. shared interaction commands (`ui.cancel`, `ui.confirm`, selection and text
   primitives), interpreted by the active input target;
2. product commands (`session.tree.open`, `message.followUp`,
   `notification.dismissVisible`), translated directly to a root action.

The catalog does not become a second reducer or host command registry.

## Scope stack

```rust
pub struct ScopeStack(Vec<ActiveScope>);

pub struct ActiveScope {
    pub kind: ScopeKind,
    pub propagation: Propagation,
    pub text_sink: Option<TextSink>,
}
```

`FocusManager` and transient UI state build the stack once per event. Example:

```text
suggestions visible: [Suggestions, Editor, Workspace, Application]
tree open:           [Tree, SelectionSurface, Application]
approval pending:    [Approval]                 // blocking
chat idle:           [Editor, Timeline, Workspace, Application]
```

A blocking scope terminates propagation by construction. There is no separate
"global Esc/Enter" stage. Truly global commands appear in the Application
scope, and their catalog enablement can exclude blocking states.

Each focus owner implements a narrow adapter:

```rust
pub trait InputTarget {
    fn command(&self, command: CommandId, context: &BindingContext) -> Option<Action>;
    fn insert_text(&self, text: &str) -> Option<Action>;
}
```

This replaces surface identity branches in the root router. Shared surface
profiles may reuse one adapter; Tree and workflows may own specialized ones.

## Context conditions

The initial condition grammar is an AND-list of closed atoms. `!` negates one
atom; OR is represented by two rules.

```toml
when = ["editor.multiline", "!suggest.visible"]
```

Examples of catalogued atoms:

```text
editor.empty
editor.multiline
editor.historyBrowsing
suggest.visible
turn.running
notice.visible
terminal.enhancedKeyboard
```

Command enablement and allowed scopes are mandatory upper bounds. A user rule
cannot make `editor.submit` executable inside Approval by omitting its
condition.

## Rule model and precedence

```rust
pub struct BindingRule {
    pub id: RuleId,
    pub key: KeyStroke,
    pub command: CommandId,
    pub scope: ScopeKind,
    pub conditions: Vec<Condition>,
    pub source: RuleSource,
}
```

Resolution order is structural:

1. active scopes, most specific first;
2. effective host-configured rule replacing the built-in rule with the same
   stable rule ID;
3. custom rules before untouched built-ins within the same scope;
4. exactly one enabled candidate must remain.

Different declaration order never breaks a tie. If two candidates at the same
precedence match one stroke, resolution returns `Conflict` and runs neither.

Guidance chooses one deterministic canonical key for a command in the active
scope. Multiple keys may invoke the same command, including aliases whose
conditions overlap. A key shared by different commands at the same active
scope remains a conflict and is neither dispatched nor advertised.

An `enabled = false` override is a tombstone for its rule ID. It does not
create a no-op command or consume unrelated rules.

## Host-owned configuration

The TUI settings schema adds a map so global and project layers merge by rule
ID:

```rust
pub struct KeybindingSettings {
    pub rules: BTreeMap<String, BindingRuleSetting>,
}

pub struct BindingRuleSetting {
    pub enabled: Option<bool>,
    pub key: Option<String>,
    pub command: Option<String>,
    pub scope: Option<String>,
    pub when: Vec<String>,
}
```

Built-in IDs may be partially overridden. Custom IDs require key, command, and
scope. hostd owns TOML parsing, layer merging, persistence, and projection in
the `tui` namespace. The namespace remains opaque to hostd: global and project
`tui` objects are recursively merged by JSON object key, so the rule-ID map
merges without teaching hostd the keybinding schema. The TUI owns structural
and semantic validation against its command catalog and terminal reachability.

On configuration change:

1. parse the host projection with a fallible
   `TuiConfig::try_from_hostd_settings` path;
2. compile a new immutable registry;
3. validate all default context fixtures and custom rule collisions;
4. atomically replace the active TUI config and registry only on success;
5. retain the previous config and registry and publish diagnostics on failure.

The current `unwrap_or_default` behavior is removed. One malformed binding must
not silently reset unrelated theme, editor, tree, or bottom-bar settings.

Direct TUI reads of `~/.piko/keybindings.json` and project JSON are removed.
Those old paths are outside the contract: the TUI neither reads nor detects
them, and does not translate their entries. Removing them is an intentional
clean break; host-owned `[tui.keybindings]` is the sole custom binding
authority.

## Guidance and diagnostics

Guidance queries:

```rust
registry.binding_for(command, &context, &terminal_profile)
```

The result is already ordered by reachability and preference. Components do not
hard-code labels such as `Shift+Enter`.

The resolver optionally emits a bounded trace:

```text
received: ctrl+p (press)
profile: baseline
scopes: suggestions > editor > workspace > application
suggestions/default-previous: matched → selection.previous
winner: suggestions/default-previous
```

Diagnostics also report unknown commands, invalid condition atoms, forbidden
scopes, unreachable chords, disabled rules, and conflicts. Normal input does
not log by default; tracing is opt-in and bounded.

## Default cleanup

The default registry is rebuilt rather than mechanically translating every
current `KeyAction`:

- remove duplicate registrations for PageUp/PageDown;
- remove `Ctrl+E` history overloading; use it only for line end;
- use scoped `Ctrl+N` for history-next and Sessions-specific behavior;
- express `Ctrl+C` as separate context rules for `turn.interrupt` and
  `editor.clear`, not one command with two meanings;
- remove explicit steer default where normal submit already follows the
  running-turn queue contract;
- keep unimplemented model/tree/approval commands out of the catalog;
- represent suggestion interception as the Suggestions scope;
- represent `Esc` with scoped commands: focus-owned `ui.cancel`, explicit
  approval decline, turn interrupt, and idle-workspace escape rules; never a
  global raw-key branch;
- choose one canonical default for each editor movement and deletion command;
  user overrides replace that rule; explicit aliases for the same command may
  remain active and guidance still shows the canonical key.

Exact default-rule tables land with catalog tests and update the Feature PRD
before implementation is marked reviewed.

## Testing

### Catalog tests

- stable unique command and rule IDs;
- every command dispatches or is explicitly focus-owned;
- every default rule references an allowed scope and known condition;
- no unreachable or ambiguous default rules across enumerated contexts.
- no context produces more than one active key for one command.

### Resolver matrix

- active-scope precedence and blocking propagation;
- suggestions over editor;
- shared selector behavior and specialized Tree/workflow behavior;
- custom override, disable, and conflict cases;
- repeatable versus press-only commands;
- enhanced versus baseline reachability;
- text sink fallback and modal non-leakage.

### Property tests

For generated valid registries and contexts, resolution is deterministic under
rule storage order. Key-to-command resolution is a partial function, while
command-to-key guidance selects one canonical key even when input aliases are
active. A result is exactly one command, text fallback, consumed, or a
structured conflict.

### PTY tests

`cargo test -p piko-tui --test terminal_pty -- --test-threads=1` drives
enhanced and baseline key encodings through the running binary and its
`terminal::InputNormalizer`. It asserts the same semantic commands for common
bindings, including distinct submit/newline behavior, bracketed paste, resize,
and cleanup.

## Migration sequence

1. Land Terminal Capabilities profile and normalized `KeyStroke`.
2. Introduce command catalog, scope/context types, and compiled built-in rules.
3. Route one shared selector and the editor through the resolver behind tests.
4. Migrate suggestions and blocking workflows into explicit scopes.
5. Migrate remaining surfaces and remove raw-key branches.
6. Generate guidance from the effective registry.
7. Add host-owned rule settings and keep old JSON paths outside the contract.
8. Remove `KeyAction`, `action_from_id`, and direct keybinding file reads.
9. Add PTY and conflict diagnostics verification.
10. Update the PRDs to reviewed and the design to implemented.

## Package impact

| Package | Change |
|---|---|
| `piko-tui` | command catalog, resolver, focus adapters, normalized input consumers |
| `piko-hostd` | recursively merge opaque `tui` namespace objects across settings layers |
| `piko-protocol` | none; existing generic config projection remains sufficient |
| `piko-tui-layout` | none |

## Deferred extensions

- The first implementation exposes resolver diagnostics through the existing
  diagnostics path; a dedicated binding browser is deferred.
- `Alt+Enter` remains the follow-up binding. Baseline terminals encode it as an
  Escape-prefixed Enter sequence, and enhanced terminals report it directly;
  PTY tests verify both paths. Users whose terminal or multiplexer intercepts
  it can replace that rule's chord with another reachable chord.
- Multi-stroke chords, preset keymaps, and interactive rebinding remain outside
  the initial implementation.
