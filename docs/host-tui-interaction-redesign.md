# Host TUI Interaction Architecture Redesign

Status: implementation proposal  
Baseline commit: `46ad47c`  
Primary implementation target: `packages/host-tui`  
Reference behavior: pi session tree navigation

## 1. Purpose

The Host TUI currently mixes four communication mechanisms in a single interaction:

- components call `PikoHost` directly;
- components dispatch `TuiEvent` directly;
- components call imperative methods on `TuiController`;
- components communicate through callbacks such as `onClose`.

This makes cross-component workflows sensitive to mount order and difficult to test. The session tree bug is the first reference case:

```text
Enter
→ TreeSelector.confirm()
→ close capture panel
→ mutate Host session leaf
→ reload branch
→ dispatch session_resumed
→ restore editor through an imperative setter
→ remount Timeline and Editor
```

The current regression test demonstrates the failure: pending editor text is consumed before the newly mounted textarea ref exists.

This document defines an incremental redesign based on:

- unidirectional data flow;
- typed intents, outcomes, and effects;
- Host as the durable domain-state authority;
- TUI Store as the shared UI-state authority;
- declarative rendering from Store state;
- pure components at workflow boundaries;
- atomic Host operations;
- explicit asynchronous operation state.

This is not a full rewrite. Existing reducers, store, controller, surfaces, and action services should be migrated in place.

## 2. Baseline and known failing test

At baseline commit `46ad47c`:

- formatting and TypeScript checks pass;
- TreeSelector component contract tests pass;
- `packages/host-tui/test/editor-remount.test.ts` fails;
- the failure is intentional and must become green during phase 2;
- `bun run test` therefore exits non-zero at the start of implementation.

Do not delete, skip, invert, or weaken the failing assertion.

## 3. Goals

1. A component emits a typed user intent instead of coordinating Host, Store, Controller, and sibling components.
2. Every asynchronous workflow has one application-level owner.
3. Host mutations return an atomic result sufficient to update the TUI.
4. Shared state that must survive component unmounting lives in `TuiStore`.
5. Reducers remain synchronous and side-effect free.
6. Editor restoration survives capture-panel unmount/remount.
7. Async success, failure, cancellation, duplicate submission, and stale results are explicit.
8. The tree navigation flow is covered end to end, from Enter through JSONL leaf, timeline, and textarea.
9. Normal session-tree behavior remains aligned with pi.
10. Other Host TUI workflows can migrate to the same pattern incrementally.

## 4. Non-goals

- Replacing SolidJS or OpenTUI.
- Moving Host domain state into the TUI.
- Introducing a global untyped event bus.
- Running side effects from reducers.
- Converting every local component signal into global state.
- Migrating every selector in the first patch.
- Automatically rewriting or merging historical JSONL entries.

## 5. Architectural rules

### 5.1 State ownership

| State | Owner | Examples |
|---|---|---|
| Durable domain state | Host | session entries, leaf, model/runtime configuration |
| Shared UI projection | TUI Store | transcript, timeline, editor draft, active operation |
| Input/focus infrastructure | TuiController | key normalization, focus routing, surface key ownership |
| Local visual state | Component | cursor, hover, local list selection, animation |

The same logical state must not be independently cached by multiple owners.

### 5.2 Commands and events

Use imperative names for requested actions and past-tense names for outcomes.

```ts
type TuiIntent =
  | { type: "session.tree.navigate"; entryId: string; surfaceId: string }
  | { type: "prompt.submit"; text: string };

type TuiEvent =
  | { type: "tree_navigation_started"; operationId: string; entryId: string }
  | { type: "tree_navigation_succeeded"; operationId: string; result: TreeNavigationViewResult }
  | { type: "tree_navigation_failed"; operationId: string; error: string };
```

Components emit intents. Application actions execute effects. Outcomes update the Store.

### 5.3 Dependency direction

```text
Component
  → domain-specific TUI action
    → Host port
      → Host/session implementation

Host result
  → TUI projection
    → Store event
      → Timeline / Editor / Status render
```

A workflow component must not directly depend on all of `host`, `actionSvc`, `controller`, and `store`.

### 5.4 Atomic Host operations

Avoid multi-call read-after-write workflows such as:

```ts
await host.navigateToEntry(entryId);
const entries = await host.loadBranchEntries();
const leafId = host.getLeafId();
```

The Host operation must return one consistent result.

## 6. Proposed modules

Add domain action modules under:

```text
packages/host-tui/src/actions/
  session-actions.ts
  run-actions.ts
  model-actions.ts
  resource-actions.ts
  surface-actions.ts
  types.ts
```

Start only with `session-actions.ts`. Do not migrate unrelated domains until the tree slice is complete.

Suggested interface:

```ts
export interface SessionActions {
  navigateTree(entryId: string, surfaceId: string): Promise<void>;
}
```

Construction belongs at the App composition root. Inject narrow ports:

```ts
interface SessionActionDeps {
  host: SessionHostPort;
  dispatch(event: TuiEvent): void;
  closeSurface(surfaceId: string): void;
  notify(notification: TuiNotification): void;
  nextOperationId(): string;
}
```

Do not pass the full `TuiController` into the action implementation.

## 7. Atomic Host tree-navigation contract

Replace the current editor-text-only result with an atomic domain result.

```ts
export interface TreeNavigationResult {
  status: "navigated" | "already_current";
  sessionId: string;
  oldLeafId: string | null;
  newLeafId: string | null;
  selectedEntryId: string;
  branchEntries: SessionTreeEntry[];
  editorContent?: Message["content"];
}
```

Host implementation requirements:

1. Verify that the selected entry exists.
2. Capture `oldLeafId`.
3. Apply pi navigation semantics.
4. Capture `newLeafId` from the same session instance.
5. Read branch entries after mutation.
6. Return all values as one result.
7. Do not return TUI view models from Host.

### 7.1 Pi-compatible navigation semantics

For a selected user message:

```text
new leaf = selected user parent
editor content = selected user content
active branch excludes selected user
```

For a selected non-user entry:

```text
new leaf = selected entry
no editor content
```

For the current leaf:

```text
status = already_current
no session mutation
```

The historical state in which a user entry with descendants is itself the current leaf was produced by an earlier piko bug. Do not silently merge entries. If recovery support is required, implement it as an explicitly named compatibility rule with its own tests and documentation.

## 8. TUI projection contract

`SessionActions.navigateTree()` maps the Host domain result to a view result:

```ts
export interface TreeNavigationViewResult {
  status: "navigated" | "already_current";
  sessionId: string;
  oldLeafId: string | null;
  newLeafId: string | null;
  selectedEntryId: string;
  transcript: TuiMessageViewModel[];
  editorDraft?: EditorDraftReplacement;
  surfaceId: string;
}

export interface EditorDraftReplacement {
  text: string;
  revision: number;
  source: {
    kind: "session_tree";
    sessionId: string;
    entryId: string;
  };
}
```

`entriesToTranscript()` remains a TUI-layer projection and must not move into Host.

## 9. Store changes

Extend `TuiInputState`:

```ts
export interface TuiInputState {
  focused: boolean;
  draft: string;
  revision: number;
  source?:
    | { kind: "user" }
    | { kind: "session_tree"; sessionId: string; entryId: string }
    | { kind: "queue_restore" };
}
```

Add explicit navigation state:

```ts
export interface TreeNavigationState {
  status: "idle" | "running" | "failed";
  operationId?: string;
  entryId?: string;
  error?: string;
}
```

Add it under session or a dedicated operations state object.

### 9.1 Reducer behavior

`tree_navigation_started`:

- set navigation status to `running`;
- retain current transcript and draft;
- record operation ID and selected entry.

`tree_navigation_succeeded`:

- ignore stale operation IDs;
- replace transcript;
- rebuild timeline items;
- update session leaf projection if stored;
- replace editor draft only when the navigation result contains editor content;
- increment editor draft revision;
- clear navigation error;
- mark navigation idle.

`tree_navigation_failed`:

- ignore stale operation IDs;
- retain transcript and editor draft;
- store the error;
- mark navigation failed.

Do not reuse `session_resumed` for branch navigation. That event should retain session-resume semantics.

## 10. Editor redesign

Remove cross-mount text transport from `TuiController`:

```ts
pendingEditorText
editorTextSetter
setEditorTextSetter()
setEditorText()
```

Keep the text accessor only if input routing genuinely requires it. Prefer reading shared draft state when possible.

The Editor receives draft state declaratively:

```tsx
<Editor
  draft={state.input.draft}
  draftRevision={state.input.revision}
  onDraftChange={(text) => dispatch({ type: "editor_draft_changed", text })}
/>
```

Because OpenTUI textarea is imperative, isolate synchronization inside `Editor`:

```ts
let lastAppliedRevision = -1;

createEffect(() => {
  const revision = props.draftRevision;
  if (!textareaRef || revision === lastAppliedRevision) return;
  textareaRef.setText(props.draft);
  textareaRef.cursorOffset = props.draft.length;
  textareaRef.requestRender();
  lastAppliedRevision = revision;
});
```

The ref callback must also apply the latest draft so initial mount is correct:

```ts
ref={(el) => {
  textareaRef = el;
  applyLatestDraft();
}}
```

Requirements:

- draft survives capture-panel unmount/remount;
- pending text is never consumed before a ref exists;
- internal textarea changes dispatch `editor_draft_changed`;
- external replacement does not create an update loop;
- empty-string replacement is supported;
- attachments are not silently retained when replacing a draft unless explicitly intended.

## 11. Surface behavior

TreeSelector should become a view component:

```ts
export interface TreeSelectorProps {
  entries: FlattenedTreeItem[];
  selectedId?: string;
  loading: boolean;
  onSelect(entryId: string): Promise<void>;
  onCancel(): void;
}
```

It must not call Host, dispatch session events, or write editor state.

The session action owns the workflow. The selected surface ID is passed with the intent so the action can close the correct surface.

For pi alignment, close the selector when a valid non-current selection is accepted. Define behavior explicitly for failures:

- precondition failure: keep surface open;
- Host mutation failure: either reopen with prior selection or close and show error, matching pi where practical;
- already-current: close and notify `Already at this point`;
- success: close, update branch, restore draft.

Do not show `Navigated to entry` for an `already_current` result.

## 12. Async and concurrency rules

1. `onConfirm` must return or await a Promise.
2. A surface in submitting state must reject duplicate confirms.
3. Every async navigation receives an `operationId`.
4. Reducers ignore stale outcomes.
5. Session switch/new/import cancels or invalidates active navigation operations.
6. Errors must not partially update transcript or editor.
7. Host success followed by TUI projection failure must surface an error and allow the canonical branch to be reloaded.

Suggested action flow:

```ts
async function navigateTree(entryId: string, surfaceId: string): Promise<void> {
  const operationId = nextOperationId();
  dispatch({ type: "tree_navigation_started", operationId, entryId });

  try {
    const domainResult = await host.navigateToEntry(entryId);
    const result = projectTreeNavigation(domainResult, surfaceId);
    dispatch({ type: "tree_navigation_succeeded", operationId, result });
    closeSurface(surfaceId);
    notifyForNavigationStatus(result.status);
  } catch (error) {
    dispatch({
      type: "tree_navigation_failed",
      operationId,
      error: error instanceof Error ? error.message : String(error),
    });
    notifyNavigationFailure(error);
  }
}
```

Exact close ordering may be adjusted to match pi, but it must be tested and centralized here rather than inside TreeSelector.

## 13. Reference implementation sequence

### Phase 0: Preserve characterization tests

- Keep `editor-remount.test.ts` failing initially.
- Keep TreeSelector boundary tests.
- Add test helpers for valid theme configuration to eliminate `Invalid hex color` noise.
- Record baseline test result in the implementation PR description.

### Phase 1: Atomic Host result

Files:

- `packages/host-runtime/src/session/session-manager.ts`
- `packages/host-runtime/src/host/session/controller.ts`
- `packages/host-runtime/src/host/index.ts`
- relevant host-runtime tests

Tasks:

1. Define/export `TreeNavigationResult`.
2. Return branch entries and leaf IDs atomically.
3. Preserve pi behavior.
4. Add root, non-root, current leaf, missing entry, structured content, and failure tests.

### Phase 2: Store-owned editor draft

Files:

- `packages/host-tui/src/state/state.ts`
- `packages/host-tui/src/state/events.ts`
- `packages/host-tui/src/state/reducers/`
- `packages/host-tui/src/renderer/opentui/Editor.tsx`
- `packages/host-tui/src/runtime/tui-controller.ts`

Tasks:

1. Add `draft`, `revision`, and optional source to input state.
2. Add draft change/replacement events.
3. Make Editor synchronize Store draft after ref creation.
4. Remove pending setter bridge.
5. Make `editor-remount.test.ts` pass without weakening it.

### Phase 3: SessionActions

Files:

- new `packages/host-tui/src/actions/session-actions.ts`
- App composition root
- action tests

Tasks:

1. Implement operation IDs and typed outcomes.
2. Call atomic Host navigation.
3. Project entries to transcript.
4. Dispatch navigation outcomes.
5. Handle notification and surface close centrally.

### Phase 4: Pure TreeSelector

Files:

- `packages/host-tui/src/renderer/opentui/select/TreeSelector.tsx`
- `packages/host-tui/src/renderer/opentui/panels/PanelBody.tsx`
- TreeSelector tests

Tasks:

1. Remove Host, ActionService, and Controller workflow dependencies.
2. Emit only selected entry ID.
3. Await selection Promise.
4. Add submitting guard.
5. Preserve list/filter rendering and keyboard behavior.

### Phase 5: Vertical integration test

Add a test using:

- real temporary `SessionManager`/`PikoHost`;
- real `TuiStore`;
- real `TuiController` where still required;
- OpenTUI Solid `testRender`;
- real capture panel lifecycle.

The test must:

1. Create `user → assistant` session entries.
2. Mount the App or the smallest production composition containing Surface, Timeline, and Editor.
3. Open the session tree.
4. Select the user and simulate Enter.
5. Wait for the operation to settle.
6. Assert the persisted JSONL leaf target equals the user's parent.
7. Assert the surface is closed.
8. Assert timeline excludes the selected user and its former descendants.
9. Assert the real textarea contains the selected user text.
10. Submit the restored text.
11. Assert a new pi-compatible user branch was created.

Do not replace this with mocks of `setEditorText()` or `dispatch()`.

### Phase 6: Migrate other workflows incrementally

After the reference slice is green, audit and migrate:

1. resume/fork/clone/import/new session;
2. model/thinking/settings selectors;
3. login/auth surfaces;
4. prompt submission, queue restore, and abort;
5. skill/template invocation;
6. tool approval.

Each migration must remove direct multi-owner coordination from the component and add an application-action test.

## 14. Required tests

### Host tree navigation

- root user moves leaf to `null`;
- nested user moves leaf to parent;
- non-user moves leaf to selected entry;
- current leaf returns `already_current` without writing a leaf entry;
- missing entry fails without mutation;
- text-array content is returned correctly;
- image content is preserved in the domain result;
- branch entries correspond to `newLeafId`;
- JSONL contains exactly one navigation leaf record on success.

### SessionActions

- dispatches started then succeeded;
- dispatches started then failed;
- does not dispatch partial success;
- closes the correct surface;
- suppresses stale outcome;
- rejects duplicate confirm while running;
- maps entries to transcript correctly;
- distinguishes navigated from already-current notifications.

### Reducers

- success replaces transcript and timeline atomically;
- success replaces editor draft and increments revision;
- failure retains transcript and draft;
- stale operation result is ignored;
- session switch invalidates navigation;
- empty branch produces empty timeline;
- empty editor text is a valid replacement.

### Editor

- pending/remounted draft appears in the actual textarea;
- mounted external replacement appears immediately;
- empty replacement clears textarea;
- latest revision wins;
- remount applies latest draft exactly once;
- user typing updates Store;
- external replacement does not loop through `onContentChange`;
- cursor moves to the expected position.

### TreeSelector

- Enter emits selected user ID;
- non-selectable rows cannot confirm;
- empty tree does nothing;
- filtering preserves valid selection;
- repeated Enter while submitting emits once;
- failure leaves a deterministic surface state;
- Escape does not mutate Host or Store.

### End to end

- nested user navigation;
- root user navigation;
- timeline truncation;
- editor restoration through capture-panel remount;
- resubmission creates a new pi-compatible branch;
- Host failure leaves old timeline/editor intact.

## 15. Acceptance criteria

The redesign is complete for the tree reference slice when:

- `bun run fmt` passes;
- `bun run check` passes;
- `bun run test` passes with no skipped regression tests;
- `editor-remount.test.ts` is green without weakening assertions;
- a real vertical integration test covers JSONL, timeline, surface, and textarea;
- TreeSelector no longer imports or receives `PikoHost`;
- TreeSelector does not dispatch `session_resumed`;
- TreeSelector does not call `controller.setEditorText()`;
- `TuiController.pendingEditorText` and setter bridge are removed;
- Host tree navigation returns an atomic result;
- navigation success/failure/current status is explicit;
- normal navigation matches pi behavior;
- no display-time deduplication or session-entry coalescing is introduced.

## 16. Review checklist

- Is every shared state field owned by exactly one layer?
- Can every asynchronous workflow be awaited?
- Can stale results be identified and ignored?
- Does a component emit intent rather than coordinate services?
- Is Host the authority for durable state?
- Is the Store the authority for cross-mount UI state?
- Are reducers pure?
- Are imperative OpenTUI APIs isolated inside adapter components?
- Does failure leave a coherent state?
- Is behavior verified at the real integration boundary rather than only through mocks?

## 17. Guidance for the implementing agent

1. Begin from commit `46ad47c`.
2. Read this document and the existing red/green characterization tests.
3. Do not delete the red editor remount test.
4. Implement phases in order and keep each phase buildable.
5. Keep changes scoped to the reference slice until the vertical test passes.
6. Preserve unrelated working-tree changes.
7. Run formatting and type checks before every commit.
8. Report intentional deviations from pi explicitly; do not hide them in rendering logic.

