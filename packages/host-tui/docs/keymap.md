# Keymap System

The keymap system should align with pi's interaction model while remaining renderer-independent.

## Layers

Use pi-compatible layers:

- `tui.*`: generic editor/select/input actions.
- `app.*`: coding-agent actions.

## Generic TUI bindings

```ts
type TuiKeybindingId =
  | "tui.editor.cursorUp"
  | "tui.editor.cursorDown"
  | "tui.editor.cursorLeft"
  | "tui.editor.cursorRight"
  | "tui.editor.cursorWordLeft"
  | "tui.editor.cursorWordRight"
  | "tui.editor.cursorLineStart"
  | "tui.editor.cursorLineEnd"
  | "tui.editor.pageUp"
  | "tui.editor.pageDown"
  | "tui.editor.deleteCharBackward"
  | "tui.editor.deleteCharForward"
  | "tui.editor.deleteWordBackward"
  | "tui.editor.deleteWordForward"
  | "tui.editor.deleteToLineStart"
  | "tui.editor.deleteToLineEnd"
  | "tui.editor.yank"
  | "tui.editor.undo"
  | "tui.input.newLine"
  | "tui.input.submit"
  | "tui.input.tab"
  | "tui.input.copy"
  | "tui.select.up"
  | "tui.select.down"
  | "tui.select.pageUp"
  | "tui.select.pageDown"
  | "tui.select.confirm"
  | "tui.select.cancel";
```

## App bindings

```ts
type AppKeybindingId =
  | "app.interrupt"
  | "app.clear"
  | "app.exit"
  | "app.suspend"
  | "app.thinking.cycle"
  | "app.model.cycleForward"
  | "app.model.cycleBackward"
  | "app.model.select"
  | "app.tools.expand"
  | "app.thinking.toggle"
  | "app.editor.external"
  | "app.message.followUp"
  | "app.message.dequeue"
  | "app.clipboard.pasteImage"
  | "app.session.new"
  | "app.session.tree"
  | "app.session.fork"
  | "app.session.resume"
  | "app.session.togglePath"
  | "app.session.toggleSort"
  | "app.session.rename"
  | "app.session.delete"
  | "app.models.save"
  | "app.models.enableAll"
  | "app.models.clearAll"
  | "app.models.toggleProvider"
  | "app.models.reorderUp"
  | "app.models.reorderDown";
```

## Defaults

Use pi defaults where possible:

| Binding | Default |
|---|---|
| `app.interrupt` | `escape` |
| `app.clear` | `ctrl+c` |
| `app.exit` | `ctrl+d` |
| `app.model.select` | `ctrl+l` |
| `app.model.cycleForward` | `ctrl+p` |
| `app.model.cycleBackward` | `ctrl+n` |
| `app.tools.expand` | `ctrl+o` |
| `app.thinking.toggle` | `ctrl+r` |
| `tui.input.submit` | `enter` |
| `tui.input.newLine` | `shift+enter` |
| `tui.input.tab` | `tab` |
| `tui.select.cancel` | `escape`, `ctrl+c` |

Model selector uses `Ctrl+L`; `Ctrl+P` and `Ctrl+N` are reserved for model cycling.

## Config

Add support for:

```text
~/.piko/keybindings.json
.piko/keybindings.json
```

Resolution order:

1. built-in defaults
2. global user keybindings
3. project keybindings
4. CLI/test overrides

## Display and hints

`KeymapManager` should provide:

- key matching
- platform display formatting, including macOS option/command labels
- conflict detection
- `keyText`
- `keyDisplayText`
- `keyHint`
- `rawKeyHint`

Visible hints must be generated from `KeymapManager`, not hardcoded component strings.
