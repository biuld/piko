# Command System

Slash commands and keyboard commands should be registered in a command registry, not hardcoded in editor or keybinding files.

## Command definition

```ts
interface CommandDefinition {
  id: string;
  slash?: {
    name: string;
    aliases?: string[];
    description: string;
    argumentHint?: string;
    getArgumentCompletions?: (prefix: string) => Promise<AutocompleteItem[] | null>;
  };
  keybindings?: KeybindingId[];
  availability?: (state: TuiState) => CommandAvailability;
  run: (ctx: CommandContext, args?: string) => void | Promise<void>;
}
```

Responsibilities:

- Provide slash autocomplete items.
- Execute slash commands.
- Execute keyboard commands.
- Report unavailable commands through notifications.
- Allow prompt templates, skills, and extension commands to register themselves.

## Existing piko gaps

Current partial slash commands:

- `/model`
- `/thinking`
- `/resume`
- `/settings`
- `/login`
- `/help`
- `/exit`

Missing piko-specific notification commands:

- `/notifications`
- `/noti`

## Pi parity commands

Register pi builtin slash commands:

- `/settings`
- `/model`
- `/scoped-models`
- `/export`
- `/import`
- `/share`
- `/copy`
- `/name`
- `/session`
- `/changelog`
- `/hotkeys`
- `/fork`
- `/clone`
- `/tree`
- `/login`
- `/logout`
- `/new`
- `/compact`
- `/resume`
- `/reload`
- `/quit`

`/notifications` is piko-specific and should be added because piko's Host/runtime split needs a place to inspect host-side notices.

These command surfaces are part of the target command/surface system:

- `ModelSelector`
- `ThinkingSelector`
- `SettingsSelector`
- `ResumeSelector`
- `LoginDialog`

## Stub policy

Register all pi commands early. Unimplemented commands must show a clear notification, not silently submit to the model.
