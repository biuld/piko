# F-52: Session trajectory inspector

> Status: redesign implemented 2026-09-13 (single trajectory view landed;
> remaining acceptance criteria are visual verification and E2E run-through)
> Priority: P1
> Source evidence: piko product decision; F-31 durable journal, F-37
> materialized read models, F-48 authoritative ModelStep boundaries, and F-51
> AgentInput work lifecycle

## Summary

piko provides a read-only TUI trajectory view for reviewing how a historical
session unfolded. The session is presented as one flat, time-ordered stream per
AgentInstance: every user input, assistant message, and tool call appears as a
role-badged row in journal order, regardless of which root AgentInput work it
belonged to. A compact lane strip above the stream visualizes ModelStep and
tool-call activity over time and supports jumping. A detail pane with tabs
(Summary, Payload, Result, Timing) shows the full content of the selected row.

This redesign replaces the previous four-lens inspector (Work / Agents /
Transcript / Journal). All removed lenses, their queries, and their DTOs are
deleted, not deprecated.

## Problem

The previous F-52 design organized history by causal work closure (root
AgentInput → ModelSteps → facts) across four lenses. Reviewing the design in
use showed:

- The primary question a developer asks is "what happened in this session, in
  order?" The Work lens forced selecting a root AgentInput before showing any
  messages, fragmenting a linear conversation into per-work closures.
- The Agents lens duplicated what an agent picker solves in one control.
- The Transcript lens overlapped the Timeline of the live session surface; for
  historical review its tree/branch detail is a niche need.
- The Journal lens is a store-debugging tool, not a product surface; the
  journal remains inspectable on disk.
- Four lenses × wide/narrow layouts × provenance filters produced ~2100 lines
  of presentation code for a read-only viewer, with the remaining acceptance
  criteria all pixel-level visual work.

## User journeys

1. A developer opens the trajectory view (defaulting to the active session),
   picks an agent from the agent selector, and reads the flat stream of that
   agent's inputs, assistant messages, and tool calls in journal order.
2. The developer scans the lane strip to find a slow or failed region, clicks
   or keys to a block, and the stream selects the corresponding row.
3. The developer opens a tool-call row and reads the detail tabs: Summary
   (status, duration, relations), Payload (arguments), Result (output or
   error), Timing (start/finish/duration, retries, fallback when diagnostic
   data exists).
4. The developer switches the agent selector to a child agent and reads its
   stream separately.
5. After restarting hostd, the same trajectory remains queryable from the
   published read models without journal replay.

## In scope

- A read-only TUI trajectory surface for current and historical sessions.
- Session selection without opening, resuming, or changing the active session.
- **Agent-sharded streams**: one flat time-ordered stream per AgentInstance,
  selected via an agent selector (root default). The selector replaces the
  former Agents lens.
- **Lane strip**: one lane each for ModelSteps and tool calls of the selected
  agent, blocks positioned/sized by recorded timing where available and by
  sequence otherwise; blocks are selectable and drive stream selection. When
  diagnostic timing is absent the strip degrades to a sequence strip and says
  so.
- **Detail tabs** for the selected row: Summary, Payload, Result, Timing.
  Tabs with no content are shown as empty with a reason (e.g. timing data is
  diagnostic and absent), never silently omitted.
- Explicit refresh only; revision-aligned snapshots; `HistoryRevisionChanged`
  handling as today.
- Cursor-paged stream rows; large bodies fetched only when a row is opened.
- Loading, empty, unavailable-detail, and integrity-error states.
- Provenance is internal: facts form the stream; diagnostics only enrich
  detail tabs and lane timing. No user-facing provenance filter.

## Out of scope

- Realtime following, polling, streaming deltas, or live updates (explicit
  refresh only).
- Mutating, resuming, forking, retrying, cancelling, or deleting from the
  surface.
- The former Transcript lens (message ancestry/tree/branch views) — deleted.
- The former Journal lens (in-product commit/event browsing) — deleted,
  including its query and DTOs.
- The former Work lens (per-root-work causal closures) — deleted; the stream
  spans works. Work grouping survives only as an internal grouping key if
  needed for lane segments.
- The former Agents lens (hierarchy browsing, inbox report flow, origins) —
  deleted; the agent selector lists agents with parent indentation and
  lifecycle only.
- User-facing provenance filtering (facts-only / diagnostics-only toggles).
- Cross-session analytics, comparison, or export.
- Search beyond the existing local filter over loaded rows.

## Behavior and states

### Navigation

The trajectory view opens as a full-body browse surface, defaulting to the
active session when one exists. It retains a breadcrumb (session · agent ·
revision). Wide layout shows stream and detail side by side; narrow layout
drill-down. The `/trajectory [session-id]` command opens the surface; the old
`/history` command is not retained as an alias. Closing restores the previous
surface and changes nothing.

### Stream

- One row per durable item of the selected agent: user inputs, assistant
  messages (text/thinking distinct), tool calls with role badges and previews.
- Rows are ordered by journal position across all works of the agent.
- Rows show: role badge, short preview, status/duration when known, and an
  indicator when diagnostic detail is attached.
- ModelStep boundaries remain in journal order but render as muted ending
  markers (`Step N ended · outcome · duration`), not as message-like rows.
- Local filter over loaded rows; filter-empty is distinct from stream-empty.
- Pagination remains automatic and does not occupy a list row with a `more`
  label.
- Pointer-wheel and trackpad input scroll the list viewport directly in
  multi-row increments. Scrolling does not fetch detail; when the selected row
  leaves the viewport, selection follows the nearest visible edge.
- Bursts of trackpad events are applied before repaint so scrolling remains
  responsive without dropping the final viewport position.
- Clicking a visible stream row selects that absolute row and opens its detail,
  including after the list has scrolled and in the wide split layout.

### Lane strip

- Two lanes: ModelSteps and tool calls of the selected agent.
- A full-width muted border separates the lane strip from the stream/detail
  region below it.
- Block width maps to recorded duration when both start and end exist;
  otherwise a fixed minimal width in sequence position. Missing timing is
  rendered distinctly, not as zero duration.
- Selecting a block selects the corresponding stream row; selecting a step
  row highlights the step's blocks.

### Detail tabs

- Summary: kind, status, outcome, identities (agent, work, step, message,
  tool call), commit revision/time. Each field is shown once; journal metadata
  is part of Summary rather than repeated below every tab.
- Payload: message content structure or input content. Tool-call payloads use
  the same expanded, tool-specific card presenter as the Timeline instead of
  exposing raw argument JSON.
- Result: tool result or error; tool-call results reuse the same Timeline card
  language and absent result shows the reason.
- Timing: start/finish/duration; retries, fallbacks, provider/model when
  diagnostic trajectory records joined by persisted identity exist. Absent
  diagnostics show "diagnostic unavailable", never fabricated values.

### Loading and errors

- Empty session shows identity and an explanation.
- Missing/stale read models rebuild from the journal before serving; the UI
  shows loading and never mixes revisions.
- Detail errors do not erase the stream; page errors keep loaded rows with
  retry guidance.
- Integrity failure is surfaced for the selected session without partial
  fallback.
- Revision-aligned inspection data may be shared in host memory and previously
  opened item details may be cached by the client for that revision. Reopening
  a cached row does not repeat storage IO or copy the full inspection snapshot.
- Formatted detail content is reused while only its viewport changes, so
  wheel/trackpad scrolling does not reformat or rewrap the full body.

## Acceptance criteria

- [ ] Selecting a session for inspection does not change the active session.
- [ ] The stream shows all durable inputs, assistant messages, and tool calls
      of the selected agent in journal order, spanning root works.
- [ ] The agent selector lists all agents (parent-indented, lifecycle
      badge) and switches streams without refetching other agents' pages.
- [ ] The lane strip renders step/tool blocks with timing when available and
      sequence otherwise; selection is bidirectionally linked with the stream.
- [ ] Detail tabs present Summary/Payload/Result/Timing; absent diagnostic
      data is labeled, not fabricated.
- [ ] Large bodies load only on open; stream pages are cursor-paged and
      revision-bound.
- [ ] After hostd restart, trajectory queries match the published snapshot
      without reading journal segments.
- [ ] The four former lenses, their commands, results, DTOs, and TUI code are
      removed; workspace fmt/clippy/tests pass.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Primary concept? | One flat per-agent time stream with a lane strip | Answers "what happened, in order" without pre-selecting work closures. |
| What is authoritative? | Required journal facts | Unchanged from the previous design; diagnostics enrich only. |
| Multiple agents? | Separate streams via an agent selector | User decision 2026-09-13; avoids interleaving unrelated agent chatter. |
| Live following? | No; explicit refresh | User decision 2026-09-13; historical review is the use case. |
| Former lenses? | Deleted (Transcript, Journal, Work, Agents) | User decision 2026-09-13; cut scope, keep one strong view. |
| Provenance filter? | Removed as user-facing control | Facts/diagnostics split stays internal for detail enrichment. |
| Work grouping? | Not a navigation level anymore | The stream spans works; grouping is internal only. |

## Reference evidence

- [F-31 durable session journal](F-31-durable-session-journal.md)
- [F-37 materialized read models](F-37-materialized-read-models.md)
- [F-48 authoritative agent lifecycle](F-48-authoritative-agent-lifecycle.md)
- [F-51 agent work lifecycle and control plane](F-51-agent-control-plane.md)
- [ADR-028 journal-derived session history](../decisions/ADR-028-journal-derived-session-history.md)
- [ADR-029 retire trajectory web viewer](../decisions/ADR-029-retire-trajectory-web-viewer.md)
