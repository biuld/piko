# Elm-ish Runtime

Selected Feature Brief: TUI architecture migration to separate state updates
from host side effects while preserving the existing slot/panel rendering model.

## Goal

The TUI uses an Elm-style boundary for runtime data flow:

- terminal input and host lines become `Msg` values
- `AppState::update` applies a `Msg` and returns `Vec<Effect>`
- the main loop executes effects against `HostdClient`
- rendering reads the current `AppState` snapshot and does not perform side effects

This keeps `hostd` communication out of reducers and makes state transitions
testable without spawning a host process.

## Data Flow

```text
HostLine / KeyEvent / Paste / Tick
        |
        v
      Msg
        |
        v
AppState::update
        |
        +-- mutates AppState
        |
        +-- returns Vec<Effect>
                      |
                      v
              main loop executes host I/O
```

`AppState` remains the root state snapshot for render/layout, but lifecycle
state is grouped by ownership:

- `SessionUiState` owns session id, turn id, pending startup/resume state, and
  pending session command ids.
- `ModelUiState` owns active model/provider/thinking state and discovered
  provider catalog data.
- feature panels continue to own their local panel state.
- filterable feature panels own their own filter text instead of sharing root
  app filter state.

The migration does not introduce dynamic panel registration or nested mini-apps
for every panel. Feature modules can still own local state and render methods;
their state updates should return root-level effects when they need host work.

## Messages

`Msg` represents external input to the app reducer:

- `Action` for keymap, paste, slash, and palette intents
- `HostLine` for decoded host stdout lines and host stream lifecycle
- `Tick` for periodic local animation/status updates

`InputRouter` continues to map raw terminal keys into `Action` so existing input
priority remains unchanged: global Esc/Enter, focused surface, editor fallback.

`Action` is only a root router over smaller intent domains. New actions should
be added to the narrow enum that owns the behavior:

- `AppAction` for process-level intents such as quit
- `EditorAction` for text editing, submit/cancel, history, and completion
- `TimelineAction` for timeline viewport movement
- `SurfaceAction` for opening/closing surfaces, selection, confirmation, and
  active surface filtering/input
- `SessionAction`, `ModelAction`, and `TreeAction` for feature-owned panel
  intents
- `ApprovalAction` and `ToolInteractionAction` for modal workflows
- `NotificationAction` for notification operations
- `SlashAction` for slash-command operations that build host commands

This keeps root `dispatch` as a delegator. Domain handlers may still call shared
helpers when behavior crosses panels, but new feature behavior should not grow a
flat `Action::Everything` list.

Command catalog entries are translated through
`action_for_command_catalog(...)` before dispatch. Palette and slash command
handling should reuse that translation table and only keep source-specific
argument parsing or palette-only behavior locally.

## Effects

`Effect` is the only reducer output that can perform I/O. The initial effect set
contains `Send(Command)` for hostd JSON-lines commands.

Reducers should optimistically update local UI state when queuing an effect. If
effect execution fails, the main loop records the error through `AppState`.

## Host Boundary

`AppState`, `dispatch`, slash parsing, and host event application do not own a
`HostdClient`. They build protocol commands as effects instead.

The main loop owns `HostdClient` and runs effects after each `Msg` update. This
keeps host communication in one place and prevents event handlers from chaining
side effects directly through a client reference.

## Rendering Boundary

Rendering remains slot/panel based:

- layout computes `LayoutMode` and slot constraints from state
- render dispatch delegates to feature panels
- root render builds narrow view structs or arguments from `AppState`
- feature panels render from their own state plus narrow props; they do not take
  `AppState`

The Elm-ish runtime does not change the no-floaters slot model.

## Feature State Boundary

Filter/search text belongs to the panel that displays it:

- `SessionList::filter`
- `ModelSelector::filter`
- `SettingsPanel::filter`
- `TreePanel::filter`
- `AuthSelector::filter`

Root input routing only appends/backspaces the active panel filter via helper
methods. Selection, confirmation, and rendering use each feature's local filter.

Local panel state machines also stay in their feature module where practical:

- `AuthSelector::confirm` turns menu/API-key state into `AuthConfirmResult`.
- `TreePanel` owns label-editor creation, cancellation, and commit extraction.
- `confirm_summary_prompt` owns the tree branch-switch summary prompt semantics.
- model and settings panels own hierarchical menu selection and confirmation.

## Migration Rules

New TUI behavior should follow these rules:

1. Convert input or host data into `Msg`.
2. Mutate app or feature state inside reducer code.
3. Return `Effect::Send` for host work instead of calling `HostdClient`.
4. Execute effects only from the main loop.
5. Keep rendering side-effect free.
6. Keep filter/search state in the feature panel that owns the list.
7. Pass view data into feature renders instead of passing root `AppState`.

Reducer implementations are split by responsibility:

- `dispatch` routes grouped `Action` domains and owns shared surface helpers plus
  overlay confirmation logic.
- `turn` owns editor submit/cancel, approval response, tool interaction, and
  completion-history updates.
- `session_ops` owns session, tree navigation, settings, and selected model
  host command construction.
- `palette` owns command catalog action execution.

Future cleanup can continue moving overlay-specific confirmation logic from
`dispatch` into tree/auth/settings reducers as those panels gain richer local
intents.
