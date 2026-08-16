# D-50: Trajectory viewer architecture

> Status: implemented (rollout 1–4; visual verification in rollout 5 is
> user-side)
> Implements: [F-36](../features/F-36-agent-run-trajectory.md) (viewer surface)

## Goal

Define the complete architecture of the hostd-served trajectory web viewer:
maintainable, zero-build-toolchain, smooth at any run size, and fully
interactive (selection, highlight, tooltip, live updates). This supersedes the
incremental patch stack that grew around the original single-file viewer.

## Context: why the current implementation is patch-on-patch

The viewer evolved through a series of corrective patches:

- Bricks started as DOM elements, were virtualized, then migrated to a canvas;
  each migration fixed one symptom (node count, per-frame DOM rebuild).
- The frozen label column moved from sticky/z-index layering to a structural
  two-column layout after scroll resets.
- Scrolling fixes (axis pinning, `overflow-anchor`, `contain`) were applied
  symptom-by-symptom; the real issue was JS-coupled scrolling (per-frame DOM
  rebuild) and nested scroll containers.
- An SSE reconnect loop re-rendered the whole tree continuously (flickering
  `#runs-strip` and `#timeline`), caused by the server emitting `reload` and
  closing streams for sessions without a recorder.
- Role colors were duplicated between CSS and canvas constants until unified
  behind CSS custom properties.
- The entire UI lives in one ~600-line HTML file with inline CSS/JS, so any
  change re-renders broad areas and resets scroll/layout.

Root causes are architectural, not cosmetic:

1. Rendering model (DOM bricks) was chosen before measuring scroll cost.
2. Layout layering was patched instead of designed (sticky → structural).
3. Scrolling was JS-coupled instead of native.
4. Visual tokens had no single source of truth.
5. There were no component boundaries, so re-renders rebuilt the whole tree.

## Architecture

### Module layout (served as static assets, no build toolchain)

Hostd serves the page and its assets as separate files (no bundler, no npm):

```text
assets/
  index.html        layout skeleton only (sessions | runs/timeline/messages)
  viewer.css        design tokens + component styles
  js/api.js         fetch + SSE wrappers, DTO mapping (camelCase)
  js/store.js       state + actions + per-slice subscriptions
  js/panels.js      sessions list + runs strip renderers
  js/messages.js    chronological message list (collapse/expand)
  js/timeline.js    canvas timeline component
  js/app.js         composition root: wiring, event delegation, SSE lifecycle
```

Routes: `GET /` (HTML) and `GET /assets/*` (CSS/JS). Hostd already serves
`/`; add static asset routes for the rest.

### State model (`store.js`)

Single immutable-ish state object with explicit actions:

```text
state = {
  sessions, runs,
  selectedSession, selectedRun,
  runDetail: { summary, assembly, records, messages },
  timeline: { items, tracks, trackItems },   // derived, pure
  selectedMessage,
}
actions: loadSessions, selectSession, selectRun, refreshRun, highlightMessage
```

Derivation stays pure: `timeline` is a function of `runDetail` (global slot
axis by `(timestamp, journal order)`, per-track grouping). Views subscribe to
slices; a change re-renders only the affected component.

### Timeline component (`timeline.js`, canvas)

The timeline is a self-contained canvas component. It owns:

- Canvas sizing with `devicePixelRatio` handling.
- Draw: visible-window culling (only bricks inside `[scrollLeft, scrollLeft +
  viewport]` are rasterized per frame).
- Hit-testing: `offsetX + scrollLeft` → slot/track → item.
- Hover tooltip (floating DOM div; canvas has no native title).
- Selection stroke on the selected brick.
- Ruler band drawn in canvas, content-aligned (ticks + timestamps scroll with
  the content; density-adaptive stride).
- Scroll handling: `requestAnimationFrame`-throttled redraw only; never
  rebuilds DOM, never touches layout.

Contract: `timeline.render(state)` is idempotent; `timeline.attach(scrollEl)`
wires scroll/pointer events once. There are no DOM nodes per brick.

Layout constants (slot width, track height, ruler height, label width,
padding) come from CSS custom properties read at runtime, so the canvas and
the DOM never drift.

### Messages and panels (DOM, native scroll)

- Messages list: normal-flow cards, native vertical scrolling, **zero JS on
  scroll**. Collapse/expand is local card state; clicking a card dispatches
  `highlightMessage`.
- Sessions list and runs strip: static DOM lists re-rendered only when their
  slices change (session list, run list / selected run).
- Runs strip chips follow a fixed information hierarchy: outcome badge →
  identity → time/duration → scale (msgs/tools/children/steps) → dropped
  warning. Zero values are omitted.

### Live updates (SSE)

- Event kinds: the session broadcast carries `Record` (a durably appended
  trajectory record, tagged with the journal revision) and `RunsChanged` (the
  session's run list changed). `RunsChanged` is published when a run starts
  (assembly record) or finishes (terminal record) and reaches every per-run
  stream regardless of the watched run, so new runs appear in the strip
  without polling or a manual refresh.
- Terminal transition: hostd appends a `trajectory.terminal` record after the
  `execution_finished` fact, so the stream pushes the running → completed /
  failed / cancelled flip and the client refetch observes the terminal
  summary (previously the viewer stayed "running" until a manual refresh).
- No-recorder sessions: `stream_run` waits on the recorder registry (a
  latest-state channel bumped when a recorder is created) instead of hanging
  on keep-alive pings forever, so a viewer opened before this process
  attached the session — e.g. right after hostd restart — picks up live
  records as soon as the first run starts.
- Client: a `Record` event triggers `refreshRun` (idempotent) → store update →
  timeline redraw + message list update; a `RunsChanged` event triggers a
  lightweight run-list reload that re-renders the strip only when the list
  changed. The refreshed run summary is also merged back into the runs strip,
  and the Refresh button reloads the session list first, so newly created
  sessions appear without a full page reload. Never a full-tree rebuild,
  never a reconnect loop.
- Streaming follow: selecting a run lands on its newest activity (timeline
  right edge, message list bottom); while records stream in the timeline
  grows its spacer/canvas/track labels (new tracks appear mid-stream) and
  keeps the right edge pinned when the user is already there, and the
  message list appends and sticks to the bottom when the user is at the
  bottom. Scrolling away stops the follow; the user is never yanked.

### Design tokens (`viewer.css`)

Single source of truth via CSS custom properties:

```css
:root {
  --role-context / --role-user / --role-assistant / --role-toolCall /
  --role-toolResult / --role-system;
  --label-w: 92px; --track-h: 34px; --ruler-h: 18px;
  --slot-w: 24px; --brick-w: 22px; --pad-x: 8px; --pad-bottom: 12px;
}
```

The canvas reads colors and dimensions through `getComputedStyle`, so
light/dark theming and dimension tuning touch one file.

## Invariants (the rules that prevent the past bugs)

1. Exactly one horizontally scrollable element (the timeline body); every
   other scroll container is native vertical with zero JS participation.
2. No DOM rebuild inside any scroll handler; canvas redraw only, rAF
   throttled, culled to the visible window.
3. Single source of truth for colors and dimensions (CSS custom properties).
4. Re-render granularity is per store slice, never the whole tree.
5. SSE is idempotent and loop-free: no-recorder sessions keep-alive only;
   `reload` only on real lag.
6. The frozen label column is structural (outside the scroller), with no
   sticky/z-index hacks.
7. All visual separators derive from tokens (for example the ruler separator
   is `--ruler-h` boundary styled by the component, not scattered inline
   borders).

## Interaction spec

- Click a brick → highlight the corresponding message (scroll into view).
- Click a message card → highlight the brick (canvas redraw) and toggle
  expand/collapse.
- Hover a brick → tooltip with timestamp + content summary.
- Run selection → runs strip chip active state + timeline/messages refresh.

## Future extensions

- Playhead and zoom: the canvas already draws in content coordinates, so a
  time-scale ruler and zoom factor are incremental.
- Run comparison and per-agent tracks: per-agent message attribution is a
  protocol change (messages are currently per-run only).
- Export run JSON and dark/light theme tokens.

## Rollout

1. Extract modules (pure refactor of the current single file; behavior and
   endpoints preserved).
2. Move all layout constants to CSS custom properties; canvas reads them at
   runtime.
3. Introduce store slice subscriptions and targeted re-renders.
4. Harden the SSE lifecycle client-side (idempotent `refreshRun`).
5. Verify: smooth scrolling on a 1,000+ message run; the web inspector shows
   no continuous DOM rebuilds and no repeated `/stream` requests; the Ruler
   separator and all separators come from tokens.
6. Workspace gates: `cargo fmt --all`, clippy with `-D warnings`, full tests.
