# OpenTUI TUI usability fix plan

This document covers four immediate usability bugs on the `bun-opentui-tui-remodel` branch after migrating from `pi-tui` to OpenTUI + SolidJS.

The goal is a small, low-risk fix pass, not a full visual redesign. These fixes should make the current TUI usable enough for further iteration.

## Local findings

Current code already contains some partial fixes:

- `Editor.tsx` already uses OpenTUI `<input>` instead of `<textarea>`.
- Slash command routing exists through `keybinding-registry.ts` and `command-dispatcher.ts`.
- `ChatView.tsx` still renders separators as text: `<text>───</text>`.
- `ActionService.shutdown()` still calls `process.exit(0)` directly.
- `runOpenTui()` still uses `render(() => <App ... />)` without owning a `CliRenderer`.

OpenTUI type check from installed packages:

- `InputRenderable` is single-line, strips newlines, and Enter submits.
- Solid `InputProps.onSubmit` is `(value: string) => void`.
- `TextareaProps.onSubmit` is `() => void`, and Textarea keybindings include both `newline` and `submit`.
- `BoxOptions.border` is `boolean | BorderSides[]`, where `BorderSides = "top" | "right" | "bottom" | "left"`.
- No local type definition exposes `borderBottom`; use `border={["bottom"]}` instead.
- `createCliRenderer()` exists in `@opentui/core` and returns a `CliRenderer` that owns terminal setup/destroy lifecycle.

## Problem 1: no visual separation between messages

### Symptom

Chat timeline messages visually run together. User, assistant, tool, branch summary, and compaction summary blocks do not have a reliable boundary.

### Cause

The old `pi-tui` implementation used dedicated border components such as `DynamicBorder`. The new OpenTUI `ChatView` currently uses a text separator:

```tsx
<text fg={theme.color("border.muted")}>───</text>
```

This has two problems:

- It only renders three cells, so it does not create a full-width message boundary.
- It depends on text rendering rather than OpenTUI's native border renderer.

### Fix

Replace text separators with a one-row `box` that draws a bottom border:

```tsx
function MessageSeparator() {
  const theme = useTheme();
  return (
    <box
      height={1}
      border={["bottom"]}
      borderColor={theme.color("border.muted")}
      paddingLeft={1}
      paddingRight={1}
    />
  );
}
```

Use `border={["bottom"]}` instead of `borderBottom`, because the installed OpenTUI types expose single-side borders through the `border` array.

### Placement policy

- Render separators between top-level timeline items.
- Do not render a separator before the first message.
- Do not render nested separators inside a tool block; tool block internals should use their own spacing/background.
- Keep the separator muted. It should structure the timeline without making every message look like a card.

### Acceptance

- At 80x24, messages have clear boundaries.
- Separators span the available chat width.
- There are no hardcoded separator strings in `ChatView.tsx`.
- Separator color comes from `theme.color("border.muted")`.

## Problem 2: editor Enter does not submit

### Symptom

User input appears in the editor, but pressing Enter does not submit a prompt.

### Cause

OpenTUI `<textarea>` is multiline by design. Its default keybindings treat Enter as `newline`, not `submit`. Attempting to override this through JSX `keyBindings` is higher risk because the Solid binding format and Textarea handler semantics differ from `<input>`.

### Fix

Use OpenTUI `<input>` for the current pass.

```tsx
<input
  ref={(el: InputRenderable) => {
    inputRef = el;
  }}
  border
  borderColor={theme.color("border.muted")}
  placeholder={disabled ? "Running..." : "/model  /thinking  /resume  /exit"}
  onSubmit={handleSubmit}
/>
```

`handleSubmit` should accept the submitted value directly:

```ts
function handleSubmit(value: string): void {
  const text = value.trim();
  if (!text) return;

  if (text.startsWith("/")) {
    const cmd = keybindings.findSlash(text);
    if (cmd) {
      inputRef?.clear();
      dispatchCommand(cmd.command, actionSvc, store);
      return;
    }
  }

  inputRef?.clear();
  actionSvc.submitPrompt(text);
}
```

### Tradeoff

This removes native multiline editing for now. That is acceptable for the immediate usability pass because:

- The TUI must first support reliable submit.
- Most coding-agent prompts are single-line commands or short instructions.
- Multiline input can return later as an explicit editor mode with tested keybindings.

### Follow-up for multiline

Do not reintroduce `<textarea>` until there is an explicit editor mode design:

- `Enter`: submit.
- `Shift+Enter`: newline.
- Paste: multiline paste should not accidentally submit.
- Visual height grows with content up to layout policy max rows.
- Tests verify keybinding behavior.

### Acceptance

- Typing `hello` and pressing Enter calls `ActionService.submitPrompt("hello")`.
- Empty input does nothing.
- Input clears after submit.
- Editor has a visible border.
- Border color comes from the theme system, not a raw hex value.

## Problem 3: slash commands do not work

### Symptom

Typing `/model` and pressing Enter does not open the model selector.

### Cause

This is the same trigger-chain failure as Problem 2. The slash command router exists, but it only runs when submit fires. If `<textarea>` consumes Enter as newline, slash command code is never reached.

### Fix

Keep slash command routing inside `<input onSubmit>`.

Expected flow:

```ts
if (text.startsWith("/")) {
  const cmd = keybindings.findSlash(text);
  if (cmd) {
    inputRef?.clear();
    dispatchCommand(cmd.command, actionSvc, store);
    return;
  }
}

actionSvc.submitPrompt(text);
```

### Required command coverage

The default registry should support at least:

- `/model`, `/m`
- `/thinking`
- `/resume`
- `/settings`
- `/login`
- `/help`, `/h`, `/?`
- `/exit`, `/quit`, `/q`

### Edge behavior

- Unknown slash commands should not silently submit to the model.
- For this pass, show a status-line error such as `Unknown command: /foo`.
- Commands marked `requiresIdle` should show a status-line warning while a stream is running.

### Acceptance

- `/model` opens the model selector.
- `/thinking` opens the thinking selector.
- `/resume` opens the resume selector when idle.
- `/exit` exits through the same safe shutdown path as `Ctrl+D`.
- Unknown slash commands do not call `submitPrompt`.

## Problem 4: terminal does not reset after Ctrl+D

### Symptom

After pressing `Ctrl+D`, the process exits but the terminal remains in raw mode. Echo and line buffering are broken until running `reset`.

### Cause

`ActionService.shutdown()` calls `process.exit(0)` directly. This bypasses OpenTUI renderer cleanup. The renderer owns terminal setup, so it must also get a chance to destroy itself.

### Fix

Make renderer shutdown an explicit dependency instead of calling `process.exit()` from the service.

In `App.tsx` or renderer entry types:

```ts
export interface AppProps {
  store: TuiStore;
  host: PikoHost;
  options?: RunTuiOptions;
  shutdown: () => void;
}
```

Create the renderer manually in `runOpenTui()`:

```ts
import { createCliRenderer } from "@opentui/core";
import { render } from "@opentui/solid";

export async function runOpenTui(
  store: TuiStore,
  host: PikoHost,
  options?: RunTuiOptions,
): Promise<void> {
  const renderer = await createCliRenderer();

  const shutdown = () => {
    renderer.destroy();
    process.exit(0);
  };

  await render(() => (
    <App
      store={store}
      host={host}
      options={options}
      shutdown={shutdown}
    />
  ), renderer);
}
```

Pass shutdown into `ActionService`:

```ts
export class ActionService {
  constructor(
    host: PikoHost,
    store: TuiStore,
    modelRegistry?: ModelRegistry,
    settingsManager?: SettingsManager,
    private readonly shutdownRuntime?: () => void,
  ) {}

  shutdown(): void {
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }

    this.shutdownRuntime?.();
  }
}
```

The service should not directly call `process.exit()` unless it is a final fallback and renderer cleanup is unavailable.

### Error-safe cleanup

Wrap render in `try/finally` so normal thrown errors also restore terminal state:

```ts
const renderer = await createCliRenderer();
let destroyed = false;

const destroy = () => {
  if (destroyed) return;
  destroyed = true;
  renderer.destroy();
};

try {
  await render(() => <App shutdown={() => { destroy(); process.exit(0); }} />, renderer);
} finally {
  destroy();
}
```

### Acceptance

- `Ctrl+D` exits and terminal echo remains normal.
- `/exit` uses the same safe shutdown path.
- Exiting while a run is active aborts first, then destroys renderer.
- Throwing during render still destroys renderer.
- `ActionService` no longer calls `process.exit()` directly in the normal path.

## Implementation order

1. Replace ChatView text separator with `box border={["bottom"]}`.
2. Confirm Editor uses `<input>` and replace raw border hex with `theme.color("border.muted")`.
3. Add unknown slash command handling.
4. Add shutdown dependency to `ActionService`.
5. Create and own `CliRenderer` in `runOpenTui()`.
6. Add a manual smoke checklist.

## Files to change

Expected minimal scope:

- `packages/host-tui/src/renderer/opentui/ChatView.tsx`
- `packages/host-tui/src/renderer/opentui/Editor.tsx`
- `packages/host-tui/src/renderer/opentui/App.tsx`
- `packages/host-tui/src/renderer/opentui/action-service.ts`
- `packages/host-tui/src/renderer/opentui/command-dispatcher.ts`
- optionally `packages/host-tui/src/state/events.ts` and `packages/host-tui/src/state/reducer.ts` for unknown-command status messages

## Manual smoke test

Run the TUI locally and verify:

```text
1. Type hello, press Enter.
   Expected: user message appears, stream starts or model error appears.

2. Type /model, press Enter.
   Expected: model selector opens.

3. Press Esc in the model selector.
   Expected: overlay closes and editor is usable.

4. Type /thinking, press Enter.
   Expected: thinking selector opens.

5. Type /unknown, press Enter.
   Expected: status line shows unknown command; no LLM request starts.

6. Press Ctrl+D while idle.
   Expected: process exits and terminal echo remains normal.

7. Start a run, then press Ctrl+C.
   Expected: run aborts, TUI remains usable.
```

## Non-goals

- Reintroducing multiline editor support in this pass.
- Rebuilding all chat/tool renderers.
- Implementing full theme loading.
- Changing Host or Engine behavior.

