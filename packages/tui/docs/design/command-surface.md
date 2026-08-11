# Slash Command Surface Design

Status: Implemented

## Modeling

`TuiCommandEntry` is the client projection of a local presentation descriptor
or `HostCommandDescriptor`. It retains `HostCommandInvoke` so inline completion
can distinguish immediate commands from argument and confirmation commands.
The projection is the sole catalog used by slash completion.

`slash_commands::SlashCommandProvider` is an `AutoCompleteProvider` and renders
only as the inline `Region::Suggest` surface.
It reuses `SelectableList`, `SelectableItem`, `PaneSpec`, and
`paint_selectable_panel`.

## Activation and results

- Immediate entries dispatch their projected action.
- Argument and confirmation entries insert the slash token for continued
  composer input.
- Invalid argument forms restore the submitted text and show usage.
- `/fork` without an entry reuses the Tree picker.
- `/login` without a provider reuses AuthSelector.
- The TUI projection intentionally omits `session.clone`,
  `auth.login-device`, and `auth.cancel-login`; these host capabilities do not
  create duplicate slash-command rows.
- `/mcp` and `/top` reuse the standard information-pane chrome and mount through
  the shared ComposerBand budget solver. `/top` projects `process.list` and
  `process.stop` into one selectable process-management journey; stop
  confirmation is panel state, not another surface.
- `/diff` and `/prompt-debug` reuse one Diagnostics surface with different
  content modes.

There is deliberately no Commands `SurfaceId`, panel state, focus route, or
keybinding action.
