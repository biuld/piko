# Autocomplete

Autocomplete should be provider-driven and independent of any specific command component.

## Provider API

```ts
interface AutocompleteItem {
  value: string;
  label: string;
  description?: string;
}

interface AutocompleteSuggestions {
  items: AutocompleteItem[];
  prefix: string;
}

interface AutocompleteProvider {
  getSuggestions(
    input: string,
    cursor: number,
    options: { force?: boolean; signal: AbortSignal },
  ): Promise<AutocompleteSuggestions | null>;

  applyCompletion(
    input: string,
    cursor: number,
    item: AutocompleteItem,
    prefix: string,
  ): { input: string; cursor: number };
}
```

## Providers

- `SlashCommandAutocompleteProvider`
- `FileAutocompleteProvider` for `@path` and explicit tab completion
- `CommandArgumentAutocompleteProvider`
- combined provider that tries slash, argument, file in order

## UI behavior

`CommandAutocomplete` owns:

- selected index
- up/down
- page up/down
- enter confirm
- tab accept
- escape cancel
- no-match state
- max visible items
- selected row styling
- scroll counter

## Surface

- `anchored(editor)` by default.
- Falls back to `insert-between` when anchored rendering cannot fit.
- Does not hide editor.
- Optional compact local hints.
- Focus is child of editor while visible.

## Slash command behavior

- `/` opens command autocomplete.
- Up/down move selection.
- Tab accepts completion.
- Enter accepts and executes slash command.
- Esc cancels autocomplete and restores editor focus.
- Unknown slash command should notify error and not submit to LLM.
