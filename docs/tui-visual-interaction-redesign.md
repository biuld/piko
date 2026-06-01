# TUI visual and interaction redesign

This document defines the next TUI redesign pass after the Bun + OpenTUI/SolidJS switch. The goal is not to copy pi's implementation mechanically, but to reach pi-level usability while keeping piko's Host + Engine split and the new OpenTUI renderer.

## Current diagnosis

The current TUI is structurally on the right track: it has a Solid app shell, layout state, reducer events, bottom bar, editor, overlays, and a chat view. The remaining problem is that these pieces still behave like a compileable skeleton rather than a product-grade terminal UI.

The main issues are visual and interaction-level:

- Chat rendering is too primitive: assistant text is plain, tool blocks are JSON snippets, diffs and command output do not have dedicated renderers, thinking/compaction/branch markers lack hierarchy.
- Styling is hardcoded in components, so the UI cannot achieve a coherent theme or adapt to terminal capabilities.
- Bottom bar, status line, and editor sizing are not yet governed by a row-budget contract; long cwd/model/session text can crowd out useful state.
- Overlays are fixed-width boxes with generic hints. They do not yet share a real selector model, focus model, preview area, or placement policy.
- Keybindings exist as local conditionals instead of a registry that can drive both behavior and visible hints.
- UI state is present, but message identity, stream reconciliation, tool expansion, scroll anchor, focus, overlay selection, and command mode need to be modeled together with layout.

## Reference from pi

Use pi as the behavioral and visual reference, especially these areas:

- `FooterComponent`: compact cwd, git branch, session name, cumulative token/cost/context usage, model and thinking level. It shows that the footer is the persistent system surface.
- `AssistantMessageComponent`: markdown rendering, thinking treatment, spacing rules, abort/error rendering, terminal prompt zones.
- `ToolExecutionComponent`: per-tool renderers, pending/success/error backgrounds, expandable state, image support, output preview, renderer shell choice.
- `ModelSelectorComponent`: searchable selector, scoped/all models, selected row, current model ordering, async loading, keyboard hints.
- `keybinding-hints.ts`: keybinding text is generated from the keybinding registry rather than duplicated strings.
- `theme/theme.ts`: semantic color tokens, markdown tokens, diff tokens, tool state backgrounds, 256-color fallback, custom theme loading.

The lesson from pi is that a strong TUI is not one big layout. It is a set of small, consistent render contracts: message contract, tool contract, selector contract, footer contract, focus contract, and theme contract.

## Product target

piko's TUI should feel like a quiet, dense coding-agent workspace:

- The transcript is the primary surface.
- The editor is always reachable unless a blocking modal owns focus.
- Persistent chrome is limited to the bottom bar.
- No top chrome by default.
- Status is short-lived and visually subordinate.
- Tool execution is readable at a glance and inspectable on demand.
- Overlays are fast command surfaces, not separate pages.
- Every visible shortcut corresponds to an actual keybinding.

## Layout model

The default layout should be:

```text
chat timeline
status line, reserved or omitted by mode
editor
bottom bar
overlay portal, modal/drawer/inline depending on viewport and task
```

Stable global information belongs in one of three places:

- Bottom bar for stable global state: cwd, branch, session, model, thinking, context, token/cost, hints.
- Timeline markers for historical state changes: model changed, session forked, compaction happened.
- Overlay title for transient local context: "Model", "Resume", "Settings", "Login".

The bottom bar is the persistent footer. It should remain attached to the bottom of the terminal and should not scroll with the transcript.

Row budgets:

- `regular`: status 1 row, editor max 8-10 rows, bottom bar 2 rows by default, optional third row for extension statuses.
- `compact`: status 0-1 row, editor max 4-5 rows, bottom bar 2 rows.
- `minimal`: no status line unless error, editor max 3 rows, bottom bar 1 row.

The implementation should stop reserving 4 bottom rows in regular mode unless there is real content. Empty reserved rows make the interface feel unfinished.

## Theme system

Introduce a real `TuiTheme` system before redesigning individual components. The theme system has two layers:

- `palette`: named raw colors, reusable across themes.
- `tokens`: semantic roles consumed by components.

Components must consume semantic tokens only. They must not read raw palette entries directly.

### File layout

```text
packages/host-tui/src/theme/
  index.ts
  schema.ts
  resolve.ts
  capabilities.ts
  palettes.ts
  themes/
    dark.ts
    light.ts
    high-contrast.ts
```

Optional user/project themes should load from:

```text
~/.piko/themes/*.json
.piko/themes/*.json
```

Project themes override global themes by name. Invalid themes should report a clear warning and fall back to the configured default theme.

### Palette layer

Palette entries are raw color values. They may be hex, 256-color indexes, or references to other palette entries.

```ts
type TuiColorValue =
  | `#${string}`
  | number
  | { ref: string };

interface TuiPalette {
  name: string;
  colors: Record<string, TuiColorValue>;
}
```

Default palette groups:

```ts
interface DefaultPalette {
  neutral0: TuiColorValue;
  neutral1: TuiColorValue;
  neutral2: TuiColorValue;
  neutral3: TuiColorValue;
  neutral4: TuiColorValue;
  neutral5: TuiColorValue;
  neutral6: TuiColorValue;
  neutral7: TuiColorValue;
  accent: TuiColorValue;
  accentMuted: TuiColorValue;
  green: TuiColorValue;
  yellow: TuiColorValue;
  red: TuiColorValue;
  blue: TuiColorValue;
  purple: TuiColorValue;
  cyan: TuiColorValue;
}
```

Palette rules:

- Keep the default palette neutral-first. The UI should not become a one-hue theme.
- `accent` is for focus/current selection, not for general decoration.
- Status colors are fixed by meaning: green success, yellow warning/pending, red error, blue informational.
- Palette names are not a component API. Changing a palette should not require changing component code.

### Semantic tokens

Tokens map semantic roles to palette values. These are the only colors components may use.

```ts
interface TuiThemeTokens {
  text: {
    primary: TuiColorValue;
    muted: TuiColorValue;
    dim: TuiColorValue;
    inverse: TuiColorValue;
    accent: TuiColorValue;
    success: TuiColorValue;
    warning: TuiColorValue;
    error: TuiColorValue;
  };
  surface: {
    base: TuiColorValue;
    selected: TuiColorValue;
    editor: TuiColorValue;
    overlay: TuiColorValue;
    toolPending: TuiColorValue;
    toolSuccess: TuiColorValue;
    toolError: TuiColorValue;
  };
  border: {
    normal: TuiColorValue;
    muted: TuiColorValue;
    accent: TuiColorValue;
    error: TuiColorValue;
  };
  markdown: {
    heading: TuiColorValue;
    link: TuiColorValue;
    linkUrl: TuiColorValue;
    inlineCode: TuiColorValue;
    codeBlock: TuiColorValue;
    codeBlockBorder: TuiColorValue;
    quote: TuiColorValue;
    quoteBorder: TuiColorValue;
    listBullet: TuiColorValue;
    rule: TuiColorValue;
  };
  diff: {
    added: TuiColorValue;
    removed: TuiColorValue;
    context: TuiColorValue;
    hunk: TuiColorValue;
  };
  tool: {
    title: TuiColorValue;
    args: TuiColorValue;
    path: TuiColorValue;
    output: TuiColorValue;
    duration: TuiColorValue;
  };
  thinking: {
    text: TuiColorValue;
    hiddenLabel: TuiColorValue;
    off: TuiColorValue;
    low: TuiColorValue;
    medium: TuiColorValue;
    high: TuiColorValue;
  };
}
```

### Theme schema

Theme JSON should allow palette reuse and semantic overrides:

```json
{
  "$schema": "https://piko.dev/schemas/tui-theme.json",
  "name": "dark",
  "extends": "builtin:dark",
  "palette": {
    "accent": "#7aa2f7",
    "neutral0": "#101216"
  },
  "tokens": {
    "text": {
      "accent": { "ref": "accent" }
    },
    "surface": {
      "selected": "#1f2937"
    }
  }
}
```

Resolution order:

1. Built-in base theme.
2. `extends` chain.
3. Theme palette overrides.
4. Theme token overrides.
5. Terminal capability conversion.

The resolver should reject cyclic references and unknown token paths.

### Terminal capability handling

The renderer should resolve colors according to terminal support:

- Truecolor terminals use hex colors directly.
- 256-color terminals convert hex to the nearest 256-color index.
- Low-color or no-color mode maps tokens to safe ANSI colors or disables color.

`NO_COLOR`, `FORCE_COLOR`, and explicit piko settings should be respected. The selected capability mode should be stored in view state for diagnostics and settings display.

### Component consumption

Expose a small API:

```ts
interface ResolvedTuiTheme {
  fg(token: TuiForegroundToken): string;
  bg(token: TuiBackgroundToken): string;
  color(token: TuiTokenPath): string | number | undefined;
  style(role: TuiStyleRole): TuiStyle;
}
```

OpenTUI components should receive already-resolved color values:

```tsx
<text fg={theme.color("text.muted")}>...</text>
<box borderColor={theme.color("border.muted")}>...</box>
```

Rules:

- No raw hex colors inside components except inside built-in theme files.
- No direct palette access in components.
- No fixed separator strings such as a hardcoded line of dashes. Use measured separators or OpenTUI border primitives.
- Every component must handle width truncation explicitly.
- Icons should be ASCII-safe where possible. Unicode check/cross/hourglass can be optional per terminal capability, but the fallback must be clean.
- Use color sparingly: accent for focus/current selection, muted for metadata, success/error/warning only for state.

### Theme tests

Add tests for:

- Built-in themes resolve without missing tokens.
- User theme overrides palette and tokens correctly.
- Unknown token paths fail validation.
- Cyclic palette refs fail validation.
- Hex colors convert deterministically in 256-color mode.
- Components or renderer files do not contain raw hex colors outside `src/theme/`.

## Chat timeline

The chat timeline needs dedicated view models keyed by stable IDs:

```ts
type ChatItem =
  | UserMessageItem
  | AssistantMessageItem
  | ThinkingItem
  | ToolCallItem
  | ToolResultItem
  | BranchSummaryItem
  | CompactionSummaryItem
  | SystemMarkerItem
  | ErrorItem;
```

Requirements:

- Assistant deltas must reconcile by `messageId`, not by "last assistant message".
- Streaming should update the active assistant item, then reconcile from the final host transcript when the turn completes.
- Auto-scroll only when `scrollAnchor === "bottom"`. If the user scrolls up, streaming must not steal the viewport.
- Tool calls are first-class timeline items with stable `toolCallId`.
- Tool blocks default to collapsed after completion if output is large; running and failed tools stay visually prominent.
- Thinking is collapsed by default, with a clear one-line placeholder and expand/collapse state.
- Branch and compaction summaries are low-chrome timeline markers, not chat bubbles.

Message rendering:

- User messages: compact labeled block, muted label, content wrapped and preserved.
- Assistant messages: markdown renderer with code fences, lists, quotes, links, and inline code.
- Code blocks: language label when known, syntax highlight when available, fallback plain render.
- Errors/aborts: rendered inline with the related assistant/tool item.

## Tool rendering

Tool rendering should follow pi's pattern: generic shell plus per-tool renderers.

Base states:

- `pending`: arguments streaming or approval waiting.
- `running`: execution started.
- `success`: result available.
- `error`: result failed.
- `aborted`: user canceled or engine aborted.

Base display:

```text
> read packages/host-tui/src/...
  42 lines
```

For known tools:

- `bash`: command, exit status, elapsed time, stdout/stderr preview, expandable full output.
- `edit/write`: file path, added/removed line counts, diff preview, expandable full diff.
- `read`: path, line range, preview line count.
- `grep/find/ls`: query/path summary, result count, preview list.
- `web/search` style tools if added later: query, source count, short result list.

Do not show raw JSON arguments as the primary UI. JSON belongs in the expanded details view.

## Editor and command model

The editor is the command center. It needs clear modes:

- `compose`: normal prompt input.
- `slash`: slash command suggestions.
- `confirm`: approval or destructive-action confirmation.
- `disabled`: streaming or modal owns focus.

Expected behavior:

- `Enter`: submit when compose input is single-line intent.
- `Shift+Enter` or configured alternative: newline.
- `Ctrl+C`: abort when running; when idle, first press shows quit hint, second press exits, or use `Ctrl+D`/`/exit` for immediate exit.
- `Esc`: close overlay or leave slash/confirm mode.
- Paste preserves multiline content without accidental submit.
- History navigation should not conflict with overlay navigation.

Slash commands should be routed through one command registry:

- `/model`
- `/thinking`
- `/settings`
- `/resume`
- `/fork`
- `/login`
- `/help`
- `/exit`
- prompt templates and skills when available

The registry should provide name, aliases, description, availability, handler, and visible hint text.

## Focus and keybindings

Replace scattered keyboard conditionals with a keybinding registry:

```ts
type FocusRegion = "editor" | "chat" | "overlay" | "confirm";
type CommandId =
  | "submit"
  | "abort"
  | "quit"
  | "openModel"
  | "openThinking"
  | "openResume"
  | "openSettings"
  | "closeOverlay"
  | "selectNext"
  | "selectPrevious"
  | "toggleExpanded"
  | "scrollUp"
  | "scrollDown";
```

The registry should drive:

- Actual keyboard handling.
- Bottom bar hints.
- Overlay footer hints.
- Help overlay.
- User-configurable keybindings later.

Only the active focus region receives region-specific commands. Global commands should be minimal: abort, quit, open command palette/help.

## Overlays

All overlays should use one shared shell with responsive placement:

- `modal`: centered, max width, max height, owns focus.
- `drawer`: full-width bottom drawer on narrow terminals, editor hidden.
- `inline`: embedded selector above editor for quick slash-command flows.

Shared selector contract:

```ts
interface SelectorState<T> {
  query: string;
  items: T[];
  filteredItems: T[];
  selectedIndex: number;
  loading: boolean;
  error?: string;
}
```

Shared selector behavior:

- Search input at top.
- Fuzzy filtering.
- Current item visually marked.
- Selection follows filtered list.
- Preview/details pane only when width allows.
- Footer hints generated from keybinding registry.
- Empty/loading/error states are explicit.

Overlay-specific requirements:

- Model selector must resolve models through `ModelRegistry`, preserve provider/model identity, show auth/provider availability, and support scoped/all model lists.
- Resume selector must actually switch the host session, then reload transcript and session metadata.
- Thinking selector should show only options valid for the current model.
- Settings selector should write through `SettingsManager` and show whether a value is global, project, or CLI-overridden.
- Login should call the actual auth flow or API-key storage path; a placeholder dialog should not ship.

## Bottom bar

The bottom bar should be the only persistent chrome. It needs a deterministic packing algorithm, not a loose flex row.

Line 1:

```text
~/project/path (branch) - session-name                         provider/model - thinking high
```

Line 2:

```text
up 12.4k  down 3.1k  cache 8.2k  $0.042  ctx 42%/200k          ^P model  ^T thinking  ^R resume
```

Compact mode:

```text
~/project - provider/model              up 12k down 3k ctx 42%   ^P model ^R resume
```

Minimal mode:

```text
~/project  provider/model  running
```

Packing rules:

- Cwd is left-truncated or middle-truncated after home abbreviation.
- Model is right-side priority but truncates before hiding critical status.
- Context/error/running state outranks cost.
- Hints are hidden before model/cwd/status.
- Git branch and session name are hidden before cwd.
- Extension statuses get a third line only when non-empty and terminal height allows it.

## UI state model

State and layout must remain coupled through explicit derived policy, not ad hoc component logic.

Required state buckets:

- Domain state: session, model, thinking, usage, auth, transcript.
- Stream state: idle/running/aborting/error, active turn id, active assistant message id.
- View state: focus region, overlay, editor draft, command mode, selector state, expanded tool IDs.
- Layout state: viewport, density, bottom bar fields, overlay placement, scroll anchor.

Layout policy should be pure and testable:

```ts
deriveLayout(domainState, viewState, viewport): TuiLayoutState
deriveBottomBarFields(state): BottomBarField[]
deriveOverlayPlacement(state): OverlayPlacement
deriveEditorRows(editorState, viewport, mode): number
```

Components should render derived state. They should not independently decide layout mode, overlay placement, or which bottom bar fields are visible.

## Implementation phases

### Phase 0: fix interaction correctness

- Stream updates keyed by `messageId`.
- Final transcript reconciliation after every run.
- Reliable idle exit path.
- Abort controller owned outside transient render action context.
- Resume selector switches host session, not only UI transcript.
- Model selector resolves complete model objects through `ModelRegistry`.
- Usage uses cumulative session usage.

This phase is required before visual polish, because broken state makes the UI impossible to judge.

### Phase 1: theme and layout primitives

- Add `theme/` module with semantic tokens.
- Replace raw component hex colors with theme tokens.
- Add measured text helpers: truncate, pad, middle-truncate, visible-width.
- Add `BottomBarPacker` with unit tests for width 40/60/80/120.
- Remove unused reserved bottom rows.

### Phase 2: interaction framework

- Add keybinding registry.
- Add focus-region routing.
- Add slash command registry.
- Wire bottom bar and overlay hints from registry.
- Add help overlay generated from command/keybinding metadata.

### Phase 3: chat and tool renderers

- Replace primitive `ChatView` with timeline item renderers.
- Add markdown/code renderer.
- Add tool shell and built-in tool renderers.
- Add expand/collapse for thinking and tool details.
- Add scroll anchor behavior.

### Phase 4: overlays

- Build shared overlay shell and selector primitive.
- Rebuild model/thinking/resume/settings/login on top of selector/dialog primitives.
- Add responsive modal/drawer/inline placement.
- Add loading/error/empty states.

### Phase 5: visual QA and parity pass

- Compare common workflows against pi: submit, stream, abort, tool run, model switch, thinking switch, resume, settings, login.
- Test terminal sizes: 120x40, 100x30, 80x24, 60x16, 40x12.
- Test long cwd, long model id, long branch, long session name, large tool output, large diff.
- Add render snapshot or structural tests for layout policies and bottom bar packing.

## Acceptance criteria

The redesign is done when these are true:

- A user can submit, stream, abort, exit, switch model, switch thinking, resume a session, open settings, and login without leaving the TUI.
- No key hint lies: every visible hint maps to an active command.
- No primary UI text overlaps or disappears unpredictably at 80x24 or 60x16.
- Long cwd/model/session values degrade by truncation, not by pushing other fields off-screen.
- Tool calls are understandable without opening JSON.
- Bash output and diffs are readable in collapsed and expanded forms.
- The transcript remains usable while streaming and does not auto-scroll when the user intentionally scrolled away from bottom.
- All colors come from theme tokens.
- Layout policy and bottom bar packing have unit tests.

## Non-goals

- Recreating pi's component library inside piko.
- Restoring the old pi TUI runtime.
- Building decorative chrome, top status areas, or marketing-style UI.
- Solving every future extension renderer before built-in tools are good.

## Recommended next task order

1. Fix stream/model/resume/exit correctness.
2. Add theme tokens and bottom bar packing.
3. Rebuild ChatView around timeline items and tool renderers.
4. Rebuild overlays around shared selector primitives.
5. Run a visual parity pass against pi workflows.
