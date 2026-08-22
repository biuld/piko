# F-42: Desktop GUI shell

> Status: partial (D-59 Slices 1–6 implemented; visual acceptance pending in V-59)
> Priority: P0
> Source evidence: piko product direction; macOS two-column desktop window
> conventions as a visual reference

## Summary

piko provides a desktop client whose primary window uses a standard two-column
shell: a floating navigation sidebar on the left, one Timeline content region
on the right, and a Composer floating above the bottom of the Timeline. The
shell keeps the conversation as the dominant surface, adapts to narrow windows
without introducing a permanent third column, and projects host-authored state
without creating a second source of product truth.

## Problem

The terminal client's stacked surfaces are optimized for constrained character
cells and do not define an appropriate desktop information architecture. A
direct translation would produce excess chrome, divide attention across too
many permanent regions, and make the Composer feel detached from the
conversation it controls. The desktop client needs a stable spatial model that
uses additional screen area without demoting the Timeline or duplicating
host-owned state.

## User journeys

1. The user opens the desktop client at a comfortable window width. A floating
   sidebar presents session and agent navigation, while the selected agent's
   Timeline occupies the rest of the window and the Composer floats near its
   lower edge.
2. The user selects another session or agent in the sidebar. The Timeline shows
   an explicit loading state, then atomically changes to the selected
   host-authored projection without showing entries from the previous target as
   current.
3. The user writes a multiline prompt. The Composer grows up to a bounded
   height and then scrolls internally; the latest Timeline content remains
   reachable and is not hidden beneath it.
4. The user narrows the window. The persistent sidebar collapses, the Timeline
   remains the primary content surface, and the user can open the same sidebar
   navigation as a temporary layer.
5. The agent streams output while the user is reading older content. The
   Timeline preserves the user's scroll position and offers a compact way to
   return to the latest content above the Composer.
6. The user closes and reopens the client. The window restores safe presentation
   preferences while the active session, Timeline, agents, usage, and pending
   actions are reconciled from the host.

## In scope

- A single primary desktop window with a two-column wide layout.
- A visually detached, floating left sidebar for session and agent navigation.
- One right-side Timeline as the dominant content surface.
- A bottom-floating Composer contained by the Timeline region.
- Responsive sidebar collapse and temporary narrow-window presentation.
- Timeline occlusion avoidance based on the current Composer footprint.
- Desktop keyboard, pointer, scroll, text selection, and focus behavior.
- Loading, empty, error, disconnected, streaming, and restored shell states.
- Host-authoritative session, agent, Timeline, foreground, usage, approval, and
  interaction projections.

## Out of scope

- A permanent inspector or third application column.
- Translating terminal rows, modal placement, or Dock Stack geometry directly
  into the desktop client.
- Defining the internal rendering of every message, thought, tool, diff, plan,
  approval, or interaction item.
- A code editor, terminal emulator, repository browser, or media workspace.
- Settings information architecture beyond a discoverable entry point.
- Exact colors, materials, shadows, radii, animation curves, pixel dimensions,
  or implementation framework APIs.
- Moving durable or user-visible authority from the host into the desktop
  client.

## Behavior and states

### Window structure

- At a comfortable width, the sidebar and Timeline are visible together as two
  columns. The Timeline receives all remaining horizontal space.
- The sidebar appears as a distinct elevated surface with visible separation
  from the window boundary and Timeline canvas. It does not visually merge into
  a full-width content pane.
- The Timeline is the only permanent content region to the right of the
  sidebar. Secondary detail expands in the Timeline or opens as a temporary
  layer rather than claiming another column.
- Window controls, navigation controls, and Timeline actions occupy predictable
  header zones without creating two competing full title bars.

### Floating sidebar

- The sidebar provides session discovery and selection, the selected session's
  agent hierarchy, current selection, and a settings entry point.
- Selecting an item changes the corresponding host-backed projection; visual
  selection alone never fabricates an active session or agent.
- The sidebar supports keyboard and pointer navigation with one clear focus
  owner and keeps the focused row visible.
- When the window cannot preserve a usable Timeline width, the sidebar leaves
  the persistent layout. A visible control opens it as a temporary layer over
  the window, and selection or explicit dismissal closes that layer.
- Returning to a wider window may restore the persistent sidebar without
  changing the selected session or agent.
- Sidebar presentation preference may be restored locally, but its session and
  agent contents always come from the current host projection.

### Timeline

- The Timeline renders the canonical selected-agent projection and preserves
  stable item identity during streaming and authoritative commit.
- Session or agent changes enter an explicit loading or empty state before the
  new projection is ready; stale content is never labeled as the new target.
- When the viewport is following the tail, new content keeps the latest item in
  view. When the user scrolls away from the tail, new content does not steal the
  scroll position and a compact return-to-latest affordance appears.
- Timeline content remains selectable and copyable without moving focus to an
  unrelated shell surface.
- The scrollable content has enough trailing space for the current Composer and
  any shell-owned floating affordance. The final item can always be scrolled
  fully above those surfaces.

### Floating Composer

- The Composer floats inside the Timeline column with visible bottom and side
  separation from the window or content boundary.
- Its width is bounded for readable composition and adapts to the available
  Timeline width; it never extends beneath the sidebar.
- Multiline input grows the Composer to a bounded maximum height, after which
  the draft scrolls internally.
- Composer height changes update Timeline trailing space without losing the
  user's draft, selection, or scroll-follow choice.
- Submit, cancel, target agent, model, thinking level, and context state are
  available from the Composer or its immediate chrome without requiring a
  permanent status column.
- Empty submission is a no-op. A failed submission preserves the draft and
  presents an actionable error. A successful accepted submission clears only
  the submitted draft.
- A disconnected or non-live session disables actions that require the host
  while keeping recoverable draft text intact.

### Focus and temporary layers

- Sidebar, Timeline, Composer, and any temporary layer have a single active
  focus owner. Opening a temporary layer transfers focus; closing it restores
  focus to the initiating surface when that surface still exists.
- Keyboard traversal reaches every primary shell action without requiring a
  pointer. Pointer interaction invokes the same product actions as keyboard
  interaction.
- Model selection, thinking selection, session actions, approvals, and user
  interactions may use temporary layers, but those layers do not permanently
  resize the two-column shell.
- Escape dismisses the top temporary layer or cancels its provisional action;
  it does not silently discard the Composer draft or exit the application.

### Loading, empty, error, and restoration

- Initial connection and session hydration have visible loading states.
- No selected session presents a session-oriented empty state with a path to
  create or open one. A live session with no Timeline entries presents a
  Composer-ready conversation empty state.
- Transport closure or decode failure is visible and cannot leave the shell
  appearing live. Recoverable drafts and local presentation preferences remain
  available.
- Host errors are associated with the operation that failed where possible;
  they do not replace unrelated Timeline content.
- Window size, position, sidebar presentation preference, and non-authoritative
  view preferences may be restored locally. Sessions, selected agent,
  Timeline, runtime status, usage, and pending actions are reconciled from
  the host before being presented as current.

## Acceptance criteria

- [ ] At a comfortable width, the primary window visibly contains one floating
      sidebar, one Timeline, and one bottom-floating Composer, with no permanent
      third column.
- [ ] The sidebar can select sessions and agents using both keyboard and
      pointer input, and the Timeline reflects the resulting host projection.
- [ ] Narrowing the window collapses the persistent sidebar before the Timeline
      becomes unusable; the same navigation remains available in a temporary
      layer.
- [ ] The Composer never overlaps the sidebar and never makes the final
      Timeline item unreachable.
- [ ] Growing and internally scrolling a multiline Composer preserves its
      draft, selection, and the Timeline's follow-versus-reading state.
- [ ] Streaming follows the tail only while the user remains at the tail; when
      scrolled away, a visible action returns to the latest content.
- [ ] Switching session or agent cannot display the previous target's entries
      as current during loading or failure.
- [ ] Disconnect, decode failure, empty session, loading, submission failure,
      and successful submission each have distinct observable states.
- [ ] Closing temporary navigation or selection layers restores focus without
      clearing an unrelated Composer draft.
- [ ] Restart restores safe window presentation preferences but reconciles all
      durable and live product state from the host.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Primary desktop information architecture | Two columns: floating sidebar plus Timeline | Keeps the conversation dominant and matches familiar desktop navigation without copying the TUI shell |
| Permanent right-side inspector | Rejected | Agent activity, tools, and details belong in the Timeline or a temporary layer; a third column divides attention |
| Composer placement | Floating inside the bottom of the Timeline column | Makes input visibly belong to the conversation while leaving the window shell stable |
| Sidebar behavior at narrow widths | Collapse to a temporary layer | Preserves Timeline usability and keeps all navigation available |
| Desktop product-state authority | Host-authored projections | Prevents GUI/TUI divergence and preserves one durable authority |
| Locally restorable state | Presentation preferences and recoverable drafts only | Window ergonomics are client-local; sessions and runtime state are not |
| TUI layout reuse | Behavioral parity where applicable, no geometric translation | Terminal slots solve different constraints from a desktop window |
| Non-product-specific desktop capabilities | Shared desktop infrastructure, not piko-private components | Reusable behavior should remain available to other desktop applications and should not fork inside the product client |

## Resolved implementation questions

1. The first release uses one adaptive product sidebar width; user resizing is
   deferred until usage establishes a useful range.
2. Pending approvals and user interactions use one Composer-adjacent attention
   layer; richer per-item rendering remains F-22 presentation work.
3. Drafts survive session/agent switches and temporary disconnects in memory.
   They do not survive a full application restart in this slice.

## Reference evidence

- F-22 client agent projection and canonical Timeline convergence.
- F-27 agent todo list distinction between live state and Timeline history.
- F-30 host-authoritative per-agent usage.
- F-38 durable selected-position behavior.
- F-40 multimodal user input and Composer attachment behavior.
- piko desktop product direction: standard two-column macOS-style window,
  floating sidebar, Timeline content region, and bottom-floating Composer.
