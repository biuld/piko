# Design Doc: Keybinding System and Input Routing

Derived from Feature Doc: [Keybindings System](../features/keybindings.md)

## Responsibilities

1. **Input Registry (`Keymap`)**:
   - Parses keyboard combinations and matches them to a symbolic `KeyAction`.
   - Loads user overrides from `~/.piko/keybindings.json` and `.piko/keybindings.json`.
2. **Context-Scoped Routing (`InputRouter`)**:
   - Dispatches parsed `KeyAction` and raw key events based on the currently active `AppMode` (focus context).
   - Resolves key collisions dynamically (e.g., `ctrl+a` moves the cursor when in `AppMode::Chat`, but selects all models when in `AppMode::Models`).
3. **Action Dispatch (`AppState`)**:
   - Takes routed `Action`s and executes the corresponding logic (editing text, switching panels, sending approvals, etc.).

## Context-Scoped Routing Design

To avoid global shortcut pollution, key actions must be resolved inside the active panel/mode context. The router matches keys in three layers:

```
                  ┌────────────────────────┐
                  │      KeyEvent /        │
                  │   Crossterm Event      │
                  └───────────┬────────────┘
                              │
                    Priority 1: Global Esc/Enter
                              │ (Handled universally)
                              ▼
                    Priority 2: Focus Owner
                              │ (Scoping based on active AppMode)
                              ├──────────────────────────┐
                              │ AppMode::Chat            │ AppMode::Models / Tree / etc.
                              ▼                          ▼
                    Priority 3: Editor       Route keys to specific panel
                                             (e.g., ctrl+a -> EnableAll)
```

### Key Handling per AppMode

The input router uses the active `AppMode` to decide how to translate a key event or a mapped `KeyAction`:

#### 1. `AppMode::Approval` (Approval Panel Focused)
Only the following keys are handled; all other keys are consumed (ignored):
- `Enter` / `KeyAction::ApprovalAccept` -> `Action::ApprovalRespond(Accept)`
- `a` / `A` / `KeyAction::ApprovalAcceptSession` -> `Action::ApprovalRespond(AcceptSession)`
- `w` / `W` / `KeyAction::ApprovalAcceptWorkspace` -> `Action::ApprovalRespond(AcceptWorkspace)`
- `p` / `P` -> `Action::ApprovalRespond(AcceptPermanent)`
- `Esc` / `ctrl+d` / `KeyAction::ApprovalDecline` -> `Action::ApprovalRespond(Decline)`

#### 2. `AppMode::Models` (Model Selector Focused)
Keys are scoped to model list operations:
- `ctrl+s` / `KeyAction::Submit` -> `Action::ConfirmSelection` (Save model config)
- `ctrl+a` -> `Action::ModelsEnableAll`
- `ctrl+x` -> `Action::ModelsClearAll`
- `ctrl+p` -> `Action::ModelsToggleProvider`
- `alt+up` / `KeyAction::SelectPrev` -> Move model up
- `alt+down` / `KeyAction::SelectNext` -> Move model down
- `Esc` / `KeyAction::Cancel` -> `Action::CloseSurface`

#### 3. `AppMode::Tree` (Session Tree Focused)
Keys are scoped to tree list navigation/manipulation:
- `ctrl+left` / `alt+left` -> Fold tree branch or move up
- `ctrl+right` / `alt+right` -> Unfold tree branch or move down
- `shift+l` -> Edit node label
- `shift+t` -> Toggle timestamps
- `ctrl+p` -> Toggle path display
- `ctrl+s` -> Toggle sorting
- `ctrl+r` -> Rename session
- `ctrl+d` -> Delete session
- `ctrl+backspace` -> Delete session noninvasively
- `ctrl+d` -> Reset filter to default
- `ctrl+t` -> Filter: hide tool results
- `ctrl+u` -> Filter: user messages only
- `ctrl+l` -> Filter: labeled entries only
- `ctrl+a` -> Filter: show all entries
- `ctrl+o` -> Filter: cycle forward
- `shift+ctrl+o` -> Filter: cycle backward
- `Esc` / `KeyAction::Cancel` -> `Action::CloseSurface`

#### 4. `AppMode::Chat` (Editor Focused)
If no overlay is visible, keys are routed to the Editor:
- Standard character keys (plain text input).
- Text editing & movement shortcuts (e.g. `ctrl+a` to move cursor to line start, `ctrl+e` to move cursor to line end).
- Submission (`Enter`).
- Global panel triggers (e.g., `ctrl+r` or `f2` to open sessions list, `f3` to open model selector, `ctrl+k` to open command palette).

## Keymap Code Mapping Updates

To support the above design, the `KeyAction` enum and defaults mapping in `packages/tui/src/input/keymap.rs` must be aligned with the feature specifications:

- **Add missing variants**: Add all missing actions such as `CursorWordLeft`, `CursorWordRight`, `DeleteWordBackward`, `DeleteWordForward`, `DeleteToLineStart`, `DeleteToLineEnd`, `Yank`, `YankPop`, `Undo`, etc.
- **Normalize defaults**: Bind `ctrl+q` for Exit (freeing `ctrl+c`), and route `ctrl+c` to `Cancel`/`Clear` / `Interrupt` globally.
- **Scoping check in dispatch**: Ensure that the routing implementation under `packages/tui/src/input/focus.rs` implements clean pattern matching for `AppMode` to apply these scoped key behaviors.
