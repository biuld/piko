# OpenTUI TUI usability fix plan

This document is the current repair plan for the `bun-opentui-tui-remodel` branch after the OpenTUI + SolidJS migration.

The goal is not another cosmetic pass. The current failures come from incomplete UX runtime modeling: selector input/navigation, focus routing, surface behavior, and timeline scroll state are split across unrelated widgets and store fields. The fix is to close those subsystem contracts, then polish visuals on top of stable behavior.

## Current findings

The latest implementation has improved basic compile-time and rendering issues, but several interaction paths are still not reliable.

1. Selector focus is modeled as native widget focus instead of a selector interaction model.
   - `/model` has a filter `<input>` and a focused `<select>`.
   - Typing should update filter while arrow keys move selection, but those two behaviors currently compete for focus.
   - `/resume` and `/settings` are still missing the same focused behavior added to `/model` and `/thinking`.

2. Blocking surface focus is only partially integrated.
   - `TuiController.openSurface()` pushes focus for blocking surfaces.
   - Surface owner currently handles only `Esc`.
   - Navigation and text input are delegated to OpenTUI native widgets, so piko's focus system does not own the actual interaction.

3. Timeline sticky scroll and pending-new-items state are disconnected.
   - `TimelineView` uses `stickyScroll` and `stickyStart="bottom"`.
   - Store state still checks `layout.chat.scrollAnchor === "manual"` to decide whether new output is pending.
   - Native sticky-scroll manual detection is not written back to `state.timeline.anchor`, `state.timeline.atBottom`, or `state.timeline.userScrolled`.

4. `LatestIndicator` is rendered inside the scrollable content.
   - When the user has scrolled away from bottom, an indicator appended at the bottom of the scrollbox is likely off-screen.
   - It should be a fixed timeline-adjacent status, not another scrolled item.

5. Pending item count is incomplete.
   - Tool calls increment pending count.
   - A newly created assistant message does not.
   - Streaming deltas should not increment count, but new top-level timeline items should.

## Target model

### Core rule

OpenTUI native controls may render pixels, but piko owns the interaction model.

For selectors this means:

- printable input updates the selector query
- `Up` / `Down` move the selected row
- `PageUp` / `PageDown` move by page
- `Enter` confirms the selected row
- `Esc` closes the selector
- focus restore is controlled by `FocusManager`

Do not split selector behavior across a focused input and a focused select. A selector is one focus owner with one controller.

## Fix 1: replace native select-driven selectors

### Problem

`ModelSelector` currently renders:

```tsx
<input onInput={...} />
<select focused options={...} selectedIndex={...} />
```

This cannot support pi-style selector behavior because filter text and list navigation fight over native focus.

### Plan

Use the existing shared selector pieces as the real implementation:

```text
packages/host-tui/src/renderer/opentui/select/
  selector-controller.ts
  selector-layout.ts
  SelectorShell.tsx
  SelectListView.tsx
```

Make each selector use a `SelectorController` focus owner:

```ts
interface SelectorController<T = unknown> {
  id: string;
  title: string;
  filterable: boolean;
  query: string;
  selectedIndex: number;
  items: SelectItem<T>[];
  visibleItems: SelectItem<T>[];
  setQuery(value: string): void;
  appendText(text: string): void;
  backspace(): void;
  move(delta: number): void;
  page(delta: number): void;
  confirm(): void;
  cancel(): void;
}
```

The rendered selector should not use OpenTUI `<select>` for command selectors. It should render rows through `SelectListView`.

### Keyboard behavior

Selector focus owner handles:

```text
Printable text  append to query
Backspace       delete from query
Up/Down         move selected row
PageUp/Down     page selected row
Home/End        jump first/last
Enter           confirm selected row
Esc             close surface
```

Filter input is a visual row, not a separate focused native input. The query can be rendered as plain text:

```tsx
<box height={1}>
  <text fg={theme.color("text.dim")}>Filter: </text>
  <text>{controller.query || "Type to filter..."}</text>
</box>
```

### Files

- `packages/host-tui/src/renderer/opentui/select/selector-controller.ts`
- `packages/host-tui/src/renderer/opentui/select/SelectListView.tsx`
- `packages/host-tui/src/renderer/opentui/select/ModelSelector.tsx`
- `packages/host-tui/src/renderer/opentui/select/ResumeSelector.tsx`
- `packages/host-tui/src/renderer/opentui/select/ThinkingSelector.tsx`
- `packages/host-tui/src/renderer/opentui/select/SettingsSelector.tsx`

### Acceptance

- `/model` opens with editor unfocused and selector focused.
- Typing `sonnet` filters models without moving focus to another widget.
- `Up` / `Down` moves the highlighted row while query remains visible.
- `Enter` selects the highlighted model.
- `Esc` closes the selector and restores editor focus.
- `/resume`, `/thinking`, and `/settings` share the same behavior.

## Fix 2: make surface focus own blocking interactions

### Problem

The current surface focus owner mostly closes on `Esc` and otherwise returns `handled: false`. That leaves real behavior to OpenTUI native widgets or global key handling.

### Plan

Surface manager should create surface state. Focus manager should own interaction dispatch.

For every blocking surface, register a `FocusOwner` whose `handleKey` delegates to the mounted surface controller:

```ts
interface SurfaceInteractionController {
  id: string;
  handleText?(text: string): boolean;
  handleKey(event: KeyEvent): FocusResult;
  focus?(): void;
  blur?(): void;
}
```

`TuiController.openSurface()` should:

1. Resolve surface.
2. Register its interaction controller.
3. Push focus if `surface.blocking === true`.
4. Keep editor focused only for `interactionOwner === "anchor"` surfaces such as slash autocomplete.

`TuiController.handleKey()` should:

1. Let emergency globals run first only when marked emergency.
2. Route to `FocusManager`.
3. If a blocking surface is active and the key was unhandled, do not run app keymap fallback.
4. Only run keymap fallback when no blocking surface is active.

### Blocking behavior

Blocking surfaces:

- selector
- menu
- form
- confirm

Non-blocking surfaces:

- editor-attached autocomplete
- status-line notification
- passive hints

### Files

- `packages/host-tui/src/runtime/tui-controller.ts`
- `packages/host-tui/src/focus/focus-manager.ts`
- `packages/host-tui/src/focus/types.ts`
- `packages/host-tui/src/surfaces/types.ts`
- `packages/host-tui/src/surfaces/surface-manager.ts`

### Acceptance

- A blocking surface prevents global keybindings from opening another surface.
- Surface-specific `Enter` confirms the surface, not editor submit.
- Surface-specific `Esc` closes the surface and restores previous focus.
- Nested surface close restores parent focus, not always editor.
- Slash autocomplete remains editor-attached and does not steal text input.

## Fix 3: rebuild slash autocomplete as editor-attached interaction

### Problem

Slash autocomplete is close to correct, but its behavior should be explicitly modeled as an editor interceptor. It should not become a blocking surface and should not use a separate focus owner.

### Plan

Keep active focus owner as `editor`.

The editor owner registers an autocomplete interceptor:

```ts
const slashAutocompleteInterceptor = {
  id: "editor.slash-autocomplete",
  priority: 100,
  match: (_event, state) => state.autocomplete?.active === true,
  handle: (event) => {
    if (event.name === "up") return move(-1);
    if (event.name === "down") return move(1);
    if (event.name === "tab") return accept();
    if (event.name === "escape") return close();
    return { handled: false };
  },
};
```

Printable text continues into the editor. The autocomplete list recalculates from editor draft.

### Required behavior

- Typing `/mo` opens suggestions.
- Continuing to type narrows suggestions.
- `Up` / `Down` changes selected suggestion.
- `Tab` inserts selected command into editor text.
- `Enter` executes the selected slash command if autocomplete is active.
- `Esc` closes suggestions and keeps editor focused.

### Files

- `packages/host-tui/src/renderer/opentui/Editor.tsx`
- `packages/host-tui/src/renderer/opentui/autocomplete/CommandAutocomplete.tsx`
- `packages/host-tui/src/runtime/tui-controller.ts`
- `packages/host-tui/src/state/reducer.ts`

### Acceptance

- Slash list can be navigated without losing editor text input.
- Accepted completion updates both Solid draft and OpenTUI input value.
- Unknown slash commands notify and do not submit to the model.

## Fix 4: connect timeline scroll state to UI state

### Problem

`stickyScroll` can keep rendering at bottom, but it does not automatically update piko state. Pending-new-items logic still depends on old `layout.chat.scrollAnchor`.

### Plan

Make timeline state the source of truth:

```ts
interface TuiTimelineState {
  anchor: "bottom" | "manual" | "item";
  atBottom: boolean;
  userScrolled: boolean;
  pendingNewItems: number;
}
```

`TimelineView` should receive callbacks:

```tsx
<TimelineView
  ...
  onScrollStateChange={(state) => {
    store.dispatch({
      type: "timeline_scrolled",
      anchor: state.atBottom ? "bottom" : "manual",
      atBottom: state.atBottom,
    });
  }}
  onJumpLatest={() => store.dispatch({ type: "timeline_jump_latest" })}
/>
```

If OpenTUI `ScrollBoxRenderable` does not expose a direct scroll callback, use the available renderable API or key-driven timeline focus mode:

- while timeline has focus, `Up/PageUp` sets `anchor = "manual"`
- `End` or jump-latest sets `anchor = "bottom"`
- sticky-scroll remains enabled only when `anchor === "bottom"`

Do not keep using `layout.chat.scrollAnchor`.

### Pending count rules

Increment `pendingNewItems` when `timeline.anchor === "manual"` and a new top-level timeline item is created:

- new assistant message item
- new tool call item
- new summary item
- important system note item

Do not increment for:

- assistant text deltas on an existing streaming item
- tool result updates on an existing tool item
- thinking deltas on an existing item

Clear `pendingNewItems` when:

- user jumps to latest
- user scrolls back to bottom
- session/transcript is replaced

### Latest indicator placement

Do not render `LatestIndicator` as the last child inside the scrollbox.

Render it outside the scrollable area, adjacent to timeline:

```tsx
<box flexDirection="column" flexGrow={1} overflow="hidden">
  <scrollbox ... />
  {pendingNewItems > 0 && (
    <LatestIndicator count={pendingNewItems} />
  )}
</box>
```

This makes it visible while the user is scrolled away from bottom.

### Files

- `packages/host-tui/src/renderer/opentui/timeline/TimelineView.tsx`
- `packages/host-tui/src/renderer/opentui/timeline/LatestIndicator.tsx`
- `packages/host-tui/src/timeline/scroll-controller.ts`
- `packages/host-tui/src/timeline/timeline-reducer.ts`
- `packages/host-tui/src/state/reducer.ts`
- `packages/host-tui/src/state/events.ts`
- `packages/host-tui/src/layout/policies.ts`

### Acceptance

- Streaming follows bottom by default.
- User scroll intervention stops auto-follow.
- New top-level items while scrolled away increment visible pending indicator.
- Assistant deltas do not inflate pending count.
- Jump latest clears pending count and restores bottom anchor.

## Fix 5: unify visual separation and surface layout

### Problem

The current UI still feels inconsistent because each surface and selector chooses its own border, spacing, list, and hint placement.

### Plan

Use a small fixed visual grammar:

- Timeline is a text flow, not a stack of cards.
- Major timeline items get quiet full-width separators.
- Blocking surfaces use one border and one hint row.
- Selector rows use the same selected-row color and truncation policy.
- Filter row, list rows, no-match row, scroll counter, and hints are budgeted before rendering.

### Surface mount rules

- `insert-between` is used for command surfaces that should appear near editor/status.
- `replace-slot` is used only when a surface fully replaces a slot.
- Avoid centered modal-style windows in TUI.
- Derived occlusion decides what underlying slots are not rendered.
- Fully covered slots should not render at all.

### Files

- `packages/host-tui/src/surfaces/surface-resolver.ts`
- `packages/host-tui/src/surfaces/surface-occlusion.ts`
- `packages/host-tui/src/renderer/opentui/surfaces/*.tsx`
- `packages/host-tui/src/renderer/opentui/select/SelectorShell.tsx`
- `packages/host-tui/src/renderer/opentui/select/SelectListView.tsx`
- `packages/host-tui/src/renderer/opentui/timeline/TimelineSeparator.tsx`

### Acceptance

- `/model` appears between status and editor, not in the middle of transcript content.
- Hints never overlap list rows.
- Borders do not create nested card stacks.
- Narrow terminal layout remains usable.

## Implementation order

1. Replace native `<select>` based command selectors with `SelectorController + SelectListView`.
2. Register selector controllers as blocking surface focus owners.
3. Remove focused native `<select>` workaround from selectors.
4. Keep slash autocomplete editor-attached and verify input/value synchronization.
5. Move timeline scroll state from `layout.chat.scrollAnchor` to `state.timeline`.
6. Render `LatestIndicator` outside the scrollbox.
7. Apply shared visual grammar to selector shell, timeline separators, and inserted surfaces.
8. Add focused tests for reducer and controller behavior.

## Test plan

Automated tests:

- selector controller filtering keeps selection clamped
- selector controller printable input updates query
- selector controller arrows/page keys update selected index
- blocking surface prevents keymap fallback
- closing child surface restores parent focus
- autocomplete interceptor handles arrows/tab/esc without stealing printable input
- timeline pending count increments only for new items while manual
- jump latest clears pending count

Manual smoke:

```text
1. /model
   Type "sonnet".
   Expected: query updates and list filters.

2. In /model
   Press Up/Down and Enter.
   Expected: highlighted row moves; Enter switches model; editor focus returns.

3. /resume
   Type a session filter, use arrows, press Esc.
   Expected: filter works; Esc closes; editor focus returns.

4. Slash autocomplete
   Type /mo, press Down, Tab, Enter.
   Expected: completion inserts into editor; Enter executes command.

5. Blocking surface
   Open /model and press a global keybinding.
   Expected: no unrelated command opens.

6. Timeline
   Start streaming, scroll away, wait for new tool/assistant item.
   Expected: timeline does not jump; visible pending indicator appears.

7. Jump latest
   Trigger jump latest.
   Expected: bottom anchor restored and pending indicator clears.
```

## Non-goals

- Reintroducing multiline editor support in this pass.
- Full theme redesign.
- Changing Host or Engine behavior.
- Adding modal-style centered TUI windows.

