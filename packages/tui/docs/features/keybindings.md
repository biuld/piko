# Keybindings and Command Routing

> Status: implemented
>
> Related: [Terminal Capabilities](./terminal-capabilities.md)

## Overview

The keybinding system maps normalized terminal input to stable TUI commands
through an ordered stack of active scopes. A command has one product meaning;
bindings decide which reachable keystrokes invoke it in a given context.
Focused components translate shared interaction commands such as confirm,
cancel, and selection movement into root application actions.

Terminal decoding and key reachability belong to Terminal Capabilities.
Keybinding resolution never receives raw Crossterm events and never branches on
a terminal brand or operating system.

## Problem

The current implementation has four overlapping sources of keyboard behavior:

1. `KeyAction` variants;
2. string action IDs in `action_from_id`;
3. raw `KeyCode` and modifier checks in `InputRouter`;
4. feature-specific key behavior embedded in surface branches.

This creates observable defects:

- the same key is registered repeatedly and then reinterpreted by router state;
- documented actions and executable actions drift;
- configuration adds or overwrites a chord but cannot reliably remove a
  default binding;
- one command cannot have several explicit context-scoped bindings in the
  current JSON object shape;
- malformed entries, unknown command IDs, and collisions are silently ignored
  or overwritten;
- guidance cannot ask one authority which binding is effective now;
- raw key checks bypass configuration and terminal capability fallbacks;
- `Shift+Enter`, `Ctrl+Enter`, and other modified keys may be advertised even
  when the active terminal path cannot distinguish them.

## User journeys

1. A user presses `Enter` in Chat and submits, then presses the same key in a
   selector and confirms the selected item. The active scope, not a raw-key
   special case, selects the command.
2. Suggestions appear above the composer. Their transient scope handles
   navigation and acceptance before the Editor scope without mutating the
   underlying keymap.
3. A baseline-capability terminal cannot reliably report `Shift+Enter`. Its
   fallback rule selects `Ctrl+J`, so newline still has exactly one active key.
4. A user changes one binding in project settings. The host applies the normal
   settings precedence, the TUI recompiles the effective rules, and unrelated
   defaults remain intact.
5. A user disables a default binding. A stable rule ID removes that rule rather
   than assigning the chord to a dummy action.
6. Two custom rules collide in the same active scope. The TUI reports the exact
   rule IDs and chord instead of selecting one based on map iteration order.
7. A user asks which command a chord invoked. Diagnostics show normalized
   input, active scopes, candidates, rejected conditions, and the winning rule.

## Concepts

### Command

A command is a stable, user-facing semantic operation such as
`editor.newline`, `ui.cancel`, `selection.next`, or `session.tree.open`.
Commands are declared once in a command catalog with:

- stable ID;
- title and description;
- allowed scopes;
- repeat policy;
- enablement predicate;
- optional terminal capability requirement;
- dispatch adapter to an existing root `Action`.

Only commands in the catalog may be configured or advertised. A command that
has no complete implementation is not registered.

### Binding rule

A binding rule connects one normalized keystroke to one command in one scope.
It may further narrow activation with closed context conditions.

```text
key + scope + conditions → command
```

Binding rules do not contain application mutation logic.

### Scope

Scopes form an ordered runtime stack:

```text
transient owner     suggestion / approval / tool interaction
focused component  tree / selector / notification list / text field
workspace owner    editor / timeline
application        true global commands
```

Resolution checks the most specific active scope first. An unhandled key may
continue to the parent scope only when the scope explicitly allows propagation.
A blocking surface never leaks input to the workspace.

### Context

Context is an immutable snapshot for one input event. It contains closed,
typed facts such as:

- active focus and transient owner;
- whether a text sink accepts text;
- suggestion visibility;
- running-turn state;
- editor empty, multiline, and history-browse state;
- command availability;
- effective terminal keyboard profile.

User rules may require context atoms or their negation. Conditions narrow a
command's catalog scope; they cannot broaden it.

## Resolution contract

For each key event:

1. Terminal Capabilities normalizes the backend event into a canonical
   keystroke and event kind.
2. Release events are ignored. Repeat events remain eligible only for commands
   whose catalog entry allows repeat.
3. The TUI captures one binding context and ordered scope stack.
4. The resolver checks rules in the first active scope containing a matching,
   enabled, reachable command.
5. One match dispatches its command.
6. Multiple matches at the same precedence are a conflict: no command runs and
   diagnostics identify every candidate.
7. If two active rules invoke the same command in the same context, that is
   also a conflict: one behavior never has two simultaneously active keys.
8. If no command matches and the scope declares a text sink, printable text is
   delivered to that sink.
9. Otherwise the key is consumed or propagated according to the scope policy.

Resolution is deterministic and independent of hash-map or file iteration
order.

## Shared interaction commands

The following commands express consistent interaction semantics and are
interpreted by the active focus owner:

| Command | Meaning |
|---|---|
| `ui.cancel` | Cancel or close the current interaction layer |
| `ui.confirm` | Confirm the focused choice or workflow step |
| `selection.previous` | Move to the previous visible choice |
| `selection.next` | Move to the next visible choice |
| `selection.pagePrevious` | Move or scroll one page backward |
| `selection.pageNext` | Move or scroll one page forward |
| `text.deleteBackward` | Delete from the active text sink |
| `completion.accept` | Accept the selected completion |

Feature-specific commands remain specific. For example, approval decline,
queue follow-up, and tree folding are not aliases for unrelated editor actions.

## Default policy

Defaults favor chords that remain stable through common terminal and
multiplexer paths. Capability-dependent rules are mutually exclusive: they
select one effective key instead of adding aliases for different commands.

### Application and workspace

| Scope | Key | Command |
|---|---|---|
| Application | `Ctrl+D` | `app.quit` |
| Workspace | `F2` | `session.tree.open` |
| Workspace | `F3` | `model.selector.open` |
| Workspace | `F4` | `agent.selector.open` |
| Workspace | `F8` | `notification.dismissVisible` |
| Timeline | `PageUp` | `timeline.pageUp` |
| Timeline | `PageDown` | `timeline.pageDown` |

Application commands that open a surface are disabled while a blocking owner
has authority. They are not implemented as an extra pre-routing priority.

### Editor

| Key | Command | Condition |
|---|---|---|
| `Enter` | `editor.submit` | editor focused |
| `Shift+Enter` | `editor.newline` | multiline and enhanced keyboard active |
| `Ctrl+J` | `editor.newline` | multiline and enhanced keyboard inactive |
| `Ctrl+P` | `editor.history.previous` | suggestions hidden |
| `Ctrl+N` | `editor.history.next` | history browse active |
| `Esc` | `ui.cancel` | suggestions visible |
| `Esc` | `turn.interrupt` | viewed agent running and suggestions hidden |
| `Esc` | `workspace.idleEscape` | editor empty, idle, and suggestions hidden |
| `Ctrl+C` | `turn.interrupt` | viewed agent running |
| `Ctrl+C` | `editor.clear` | viewed agent idle |
| `Ctrl+C` | `timeline.copySelection` | Timeline selection active |
| `Tab` | `completion.accept` | suggestions visible |
| `Shift+Tab` | `selection.previous` | suggestions visible |

`Ctrl+E` always means line end in the editor; it is not conditionally changed
into history navigation. Plain `Enter` while a turn is running follows the
authoritative message-queue behavior, so a duplicate explicit steer binding is
not required by default. Capability-dependent queue shortcuts may remain as
additional commands when their chord is reachable.

### Selection and workflows

| Key | Command |
|---|---|
| `Up` | `selection.previous` |
| `Down` | `selection.next` |
| `PageUp` | `selection.previousPage` |
| `PageDown` | `selection.nextPage` |
| `Enter` | confirm |
| `Esc` | cancel; Approval binds its explicit decline command |

Tree, approval, tool interaction, and notification operations add commands in
their own scopes. They do not hard-code raw terminal keys in the root router.

## Text input

Printable text is not represented as a command per character. After binding
resolution, the active scope's declared text sink receives normalized text.
This covers the composer, filters, labels, and workflow fields without letting
text leak through a modal barrier.

IME and keyboard-layout-produced text must be preserved. Terminal enhancement
flags that replace text with key identities are not enabled unless the backend
also preserves the associated text required by piko.

## Configuration

Bindings move into host-owned settings. The TUI does not read global or project
keybinding files directly.

```toml
[tui.keybindings.rules.default-editor-newline-fallback]
key = "ctrl+j"
command = "editor.newline"
scope = "editor"
when = ["editor.multiline"]

[tui.keybindings.rules.default-editor-history-previous]
enabled = false
```

Rules are keyed by stable rule ID so normal global/project settings merging can
override or disable one rule without replacing the whole registry. Multiple
rules for one command are valid, including explicit input aliases. Capability-
dependent rules are mutually exclusive, as with enhanced/fallback newline.
Conditions within one rule are AND. Guidance chooses one canonical key while
input resolution retains configured aliases.

The host owns persistence and layer merging. The TUI owns semantic compilation
against its command, scope, context, and terminal-capability catalogs. On an
invalid update, the TUI keeps the last valid compiled registry and surfaces a
diagnostic.

The existing `~/.piko/keybindings.json` and `.piko/keybindings.json` paths are
outside this contract. They are not read, detected, migrated, or used for
startup diagnostics. They may be removed; host-owned `[tui.keybindings]` is
the sole custom binding authority.

## Guidance and discovery

- All displayed shortcut hints are queried from the effective binding registry.
- A hint includes only bindings active in the current context and reachable
  through the current terminal profile.
- Diagnostics can list commands with no reachable binding.
- Conflict diagnostics include normalized chord, active scope, conditions,
  source rule IDs, and resolution result.
- A future binding browser may search the same command catalog; it must not
  maintain a second list.

## Acceptance criteria

- [x] `KeyAction` and `action_from_id` are replaced by one command catalog.
- [x] Production focus routing contains no raw `KeyCode` or modifier matching.
- [x] One scope-stack resolver handles application, workspace, surface,
     transient, and text-sink input.
- [x] Blocking focus owners cannot propagate keys or text to the editor.
- [x] Default rules have stable IDs and deterministic precedence.
- [x] For every reachable context, key-to-command resolution is unique;
     command-to-key guidance chooses one deterministic canonical key.
- [x] User rules can add, override, and disable bindings without dummy actions.
- [x] Unknown commands, invalid scopes, malformed conditions, unreachable
     chords, and same-precedence conflicts produce visible diagnostics.
- [x] Every registered command has an executable dispatch path and every
     advertised hint comes from an active binding rule.
- [x] `Enter` and newline remain distinct in enhanced and baseline profiles.
- [x] `Ctrl+J` inserts newline in the baseline profile.
- [x] Text input, including composed Unicode, bypasses neither modal authority
     nor capability normalization.
- [x] Repeat policy prevents repeated destructive or lifecycle commands.
- [x] Host-owned global/project settings are the sole custom binding authority.
- [x] Capability and context matrix tests cover every default-rule collision.
- [x] PTY tests verify the effective bindings seen through enhanced and baseline
     terminal input paths.
- [x] `cargo test -p piko-tui` and workspace clippy pass.

## Non-goals

- Terminal-brand-specific default keymaps.
- Vim/Emacs modal editing presets in the initial implementation.
- Arbitrary scripts or macros as binding commands.
- Multi-stroke chord sequences in the initial implementation.
- OS-global shortcuts.
- Allowing user rules to bypass blocking focus or command safety conditions.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Registry direction | Key + active scope → command | The same key legitimately means different things in different focus owners |
| Command identity | Stable catalog ID | Config, dispatch, hints, and diagnostics share one authority |
| Context language | Closed atoms with negation | Contextual power without arbitrary runtime expressions |
| Collision behavior | Reject ambiguity | Silent last-write wins is not debuggable |
| Active aliases | Allowed for one command; guidance chooses a canonical key | Input may retain explicit aliases without duplicating discoverability |
| Text input | Declared text-sink fallback | Printable text is data, not thousands of commands |
| Customization authority | hostd settings | Preserves the host-authoritative configuration model |
| Fallback newline | `Ctrl+J` | Distinct from carriage-return Enter in Crossterm raw mode |
| Old JSON | Clean break; ignored and undetected | Host-owned settings are the sole custom binding authority |
