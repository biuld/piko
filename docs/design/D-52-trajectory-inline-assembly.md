# D-52: Trajectory viewer inline assembly

> Status: implemented (viewer slice; user-side visual QA pending)
> Implements: [F-36](../features/F-36-agent-run-trajectory.md) (viewer surface)
> Decisions: none new

## Goal

Remove the trajectory viewer's `Prompt` tab and render each run's prompt
assembly as a first-class card in the chronological message stream, with the
same card behavior as ordinary messages. The timeline keeps its prompt marker
brick, and clicking it selects/highlights the assembly card exactly like
clicking a message brick selects a message. This supersedes the tab-based
layout chosen in [D-51](D-51-trajectory-prompt-and-cache-view.md) while
preserving all of its content (block list, cache plan bar, tool catalog, raw
JSON).

## Context

D-51 introduced `Conversation | Prompt` tabs and rejected a collapsible prompt
section above the timeline because long prompts push the conversation off the
page. The product decision for this slice is different: the assembly is not a
separate document for a run, it is the run's first event, and the developer
reading a run wants to see "what prompt was frozen for this run" in the same
chronological flow as everything else. Collapsing the card by default (metadata
only) keeps the first-paint cost proportional to nothing: prompt body content
is built only on first expand, matching the D-51 "content stays collapsed by
default" invariant.

The data already supports this: `TrajectoryRun.assembly` is the per-run frozen
assembly with `recorded_at` before every committed message, and
`TrajectoryRun.messages` carries committed messages. No protocol, query, or
capture change is required.

## Constraints and non-goals

- No changes to `piko-protocol`, `piko-orchd`, `piko-llmd`, or hostd Rust code:
  this is a viewer-assets slice only.
- Assembly stays per-run; there is exactly one assembly card per run view.
  Session-level aggregation across runs is out of scope.
- No build toolchain, no new HTTP endpoints, viewer stays read-only loopback.
- D-50 invariants hold: native vertical scroll with zero JS on scroll,
  per-slice re-renders only, no full-tree rebuild inside scroll or append
  paths, colors/dimensions from CSS custom properties.

## Proposed design

### Display list: one time-ordered card stream

The store's `messages` slice becomes the display list derived from the run:

```text
messageItems = [ assembly card (if run.assembly) , ...run.messages ]
               ordered by timestamp (stable; assembly precedes input commit)
```

`deriveMessageItems(run)` (exported from `messages.js`) builds the list once
per run selection/refresh. `selectedMessage` and every message index now refer
to this display list, so the assembly card is selected and highlighted through
the exact same paths as ordinary cards.

### Assembly card (messages stream)

The assembly card is a normal `.msg` card with `--role: var(--role-prompt)`:

- Collapsed preview: `prompt assembly` role label, timestamp, and one metadata
  line (`v5 · source digest short · N blocks · N tools · cache policy`).
- Click: toggles expand and selects the card (identical to other cards).
- First expand: builds the body lazily from the D-51 pure derivation
  (`derivePrompt`) and renderers — assembly header, cache plan bar, block list
  with badges/source/expandable content/filter chips, tool catalog, raw JSON —
  into the card's full-content container. No prompt body is constructed before
  first expand.
- Selection from the timeline highlights the card and scrolls it into view.

### Timeline

`deriveTimeline(run, messageItems)` derives over the display list:

- The prompt marker brick (`kind: "prompt"`, label `prompt assembled`, time =
  `assembly.recordedAt`) carries `ref: { kind: "message", index: <assembly
  index> }` — the same ref shape as message bricks. Clicking it selects the
  assembly card; there is no tab to switch to.
- Message and step bricks index into `messageItems` (step bricks resolve their
  assistant message by `messageId` against the display list), so selection
  never drifts when the assembly card is present.
- The `onSelectPrompt` callback and prompt-tab click branch are removed.

### Removals

- `index.html`: `#run-tabs` and `#prompt-view` are deleted; the conversation
  view (run stats, timeline, messages) is the only run-detail surface.
- `app.js`: `activeTab`, `applyTab`, `selectTab`, `onSelectPrompt`, and the
  `tab:selected` subscription branch are removed; run selection stores the
  derived `messageItems`.
- `viewer.css`: `.tab` / `#run-tabs` rules are removed; prompt section styles
  (`prompt-section`, cache bar, block cards, tool cards, raw JSON) are
  retained because they now render inside the card.
- `prompt.js`: `createPrompt()` becomes container-parameterized
  (`createPrompt(container)`), used once per assembly card on first expand;
  `derivePrompt` and all section renderers are unchanged.

### Live refresh

`messages.append` keeps its tail-append path for new messages. The only
non-tail change is the assembly card appearing/disappearing at the front (the
initial fetch can race the assembly record); when assembly presence changes,
the card list re-renders once (D-50 invariant 4), otherwise new messages append
without a rebuild.

## Package impact

| Package | Change |
|---|---|
| `piko-hostd` | Viewer assets only: `index.html`, `js/app.js`, `js/timeline.js`, `js/messages.js`, `js/prompt.js`, `viewer.css` |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- A run without an assembly record renders no assembly card and keeps its
  `prompt` marker brick absent — the existing "no assembly" fallback.
- Oversized prompt content is handled at render time: collapsed by default,
  block content expandable with its own scroll (F-36/D-51 invariant); stored
  records are never altered.
- Live refresh races: assembly presence change triggers a single full card
  re-render; transient fetch errors keep the stream retrying (existing
  behavior).

## Verification

- `node --check` on every changed JS module.
- Fixture-driven manual browser verification against a fake run: the assembly
  card renders at the head of the message stream ordered by time, expands its
  full prompt content on first click, the timeline prompt brick selects and
  scrolls the card, step-brick selection still lands on the correct assistant
  message, and a live refresh with a new message appends without a full-tree
  rebuild.
- Regression check on a real hostd trajectory run (old journals without
  assembly, runs with/without `usage`) via the loopback viewer.
- Workspace gates: `cargo fmt --all`, clippy with `-D warnings`, full tests
  (Rust surface unchanged).

## Alternatives considered

- **Keep the Prompt tab** — superseded: the assembly is the run's first event,
  not a separate document; a tab hides it from the chronological flow the
  developer is already reading.
- **Collapsible prompt section pinned above the timeline** — rejected in D-51
  and again here: it breaks the card-stream chronology and the
  "assembly is a message-like event" model the user chose.
- **Drawer / side panel** — rejected: narrow content column for long blocks,
  and the same separation-from-stream problem.
- **Session-level assembly timeline** — out of scope for this slice: runs are
  individually fetched; a cross-run aggregate needs a new query surface.

## Rollout

1. Docs (this design + F-36 viewer bullet).
2. Viewer implementation: display-list derivation, assembly card, timeline
   prompt-brick selection, removals.
3. Verification: syntax checks, fixture-driven browser pass, regression smoke
   against a live run; update V-49 notes.
