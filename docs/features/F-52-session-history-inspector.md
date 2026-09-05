# F-52: Session history inspector

> Status: in progress
> Priority: P1
> Source evidence: piko product decision; F-31 durable journal, F-37
> materialized read models, F-48 authoritative ModelStep boundaries, and F-51
> AgentInput work lifecycle

## Summary

piko provides a read-only TUI inspector for understanding how a durable
session was composed. It organizes history by Session, AgentInstance, root
AgentInput work, ModelStep, message, and tool relations, while preserving the
journal's commit order and atomic boundaries. Required journal facts form the
authoritative history. Best-effort trajectory observations enrich that history
with prompt assembly, provider timing, retries, fallbacks, and intermediate
tool status without replacing or contradicting the facts.

## Problem

The existing session surfaces answer separate operational questions:

- Resume Session finds and opens a session.
- Session Tree navigates the committed conversation branch.
- Timeline renders the selected agent's conversation.
- The retired trajectory web viewer displayed a best-effort diagnostic record
  organized around an older run-oriented model.

None of them explains the current durable domain model as one coherent history.
A developer cannot inspect one historical session and reliably answer:

- which AgentInput started a unit of work, which inputs steered it, and which
  follow-ups remained queued or were cancelled;
- which assistant message and ordered tool declarations belong to each
  ModelStep;
- which tool results, pending actions, interrupts, reports, and usage facts
  belong to the same causal root;
- how agents, roots, transcript ancestry, branches, compactions, and commits
  relate;
- which displayed information is authoritative and which is optional
  diagnostic capture.

The trajectory viewer could not be promoted to that role. Trajectory records are
intentionally optional and may be dropped, while the journal's required facts
are validated, replayable authority. A new history surface must begin from the
journal model rather than infer authority from trajectory timestamps.

## User journeys

1. A developer opens Session History, selects a session without resuming it,
   and sees its agents and root AgentInput work newest first.
2. The developer opens one root and follows its input admission, processing
   boundaries, ordered ModelSteps, assistant messages, tool declarations and
   results, steers, pending actions, interrupts, usage, and terminal outcome.
3. The developer switches to Agents and follows parent/child AgentInstance
   relationships, detached work, and inbox reports without introducing Run or
   Execution as product concepts.
4. The developer switches to Transcript and sees message ancestry, session-tree
   branches, branch selection, compaction, and summaries independently from
   causal work grouping.
5. The developer switches to Journal and inspects revision-ordered commits and
   their ordered facts, including which facts were committed atomically.
6. When matching trajectory observations exist, the developer expands a prompt
   assembly, retry, fallback, provider timing, or intermediate tool-status
   detail. The surface labels it diagnostic and remains usable when it is
   absent.
7. After restarting hostd, the same published history remains queryable without
   replaying the journal on the ordinary read path.

## In scope

- A read-only TUI Session History surface for current and historical sessions.
- Session selection without opening, resuming, attaching, or changing the
  active session.
- Four history lenses:
  - **Work**: root AgentInput causal closures and their ModelSteps, messages,
    tools, controls, usage, and outcome.
  - **Agents**: AgentInstance hierarchy, work, caller relations, and inbox
    report flow.
  - **Transcript**: message ancestry, session tree, selected branch history,
    compaction, and branch summaries.
  - **Journal**: revision-ordered commits with ordered event summaries and
    visible atomic commit boundaries.
- Explicit provenance for every item: authoritative required fact or optional
  diagnostic observation, independently from whether referenced detail is
  available.
- Cursor-paged lists and lightweight item summaries; full large content is
  fetched only when the user opens that item.
- A durable, write-time history read model. Normal queries do not replay or
  scan journal segments.
- Best-effort trajectory enrichment for prompt assembly, provider request
  metadata, retries, fallbacks, timing, and intermediate tool-call states.
- Stable generic presentation for recognized-but-not-specialized history items
  so a new event kind does not make the whole surface unusable.
- Loading, empty, unavailable-detail, integrity-error, and stale/rebuild states.
- Retirement of the F-36 loopback HTTP/SSE trajectory viewer (ADR-029).

## Out of scope

- Realtime following, polling, streaming deltas, animation, or an SSE client.
- Mutating, resuming, forking, navigating, retrying, cancelling, or deleting a
  session from the history surface.
- Treating Turn, Run, or Execution as product identities.
- Treating trajectory records, timestamps, or adjacency as durable authority.
- Reconstructing model streaming deltas that were never persisted.
- Guaranteeing diagnostic content when best-effort capture dropped a record.
- Cross-session analytics, run comparison, evaluation datasets, or export.
- Showing secret authentication material or unredacted provider transport
  payloads that are not part of the durable content policy.
- Reviving a loopback HTTP/SSE trajectory viewer. ADR-029 retires that surface;
  Session History is the inspector.

## Behavior and states

### Navigation

Session History opens as a full-body browse surface. It defaults to the active
session when one exists and otherwise opens the normal session selector. A
session chosen for inspection remains separate from the active session.

The surface retains a breadcrumb through Session, lens, work or agent, and
selected item. Wide terminals may show selection and detail panes together;
narrow terminals use drill-down navigation with the same state and commands.

Closing Session History restores the previous TUI surface and does not change
the inspected or active session.

### Work lens

One work row represents the causal closure of an AgentInput whose disposition
became `applied_as_root`. The row shows its origin, target agent, input preview,
start/finish times, outcome, ModelStep/tool/message counts, and effective usage.

Work detail is ordered by authoritative journal position. It includes related
steers and their applied ModelStep, processing boundaries, ModelSteps, committed
messages, tool declarations and results, pending-action transitions, interrupt
intent, reports, usage facts and corrections, and terminal outcome.

The display groups related facts for readability but always exposes their
revision and does not invent a lifecycle event that was not recorded.

### Agents lens

The Agents lens shows stable AgentInstance parentage and lifecycle. Work is
grouped under its target agent. Caller and report relations appear when the
journal contains their stable causal identifiers. Missing legacy causation is
shown as unavailable rather than inferred from timing.

### Transcript lens

The Transcript lens is independent from Work grouping. It shows agent-private
message ancestry and the host session tree, including off-branch entries,
branch selections, compactions, branch summaries, and the current selected
position. A message may link back to its root work and ModelStep.

### Journal lens

The Journal lens orders commits by durable revision and events by their order
inside the commit. A commit is rendered as one atomic group with commit time,
producer, causation/correlation identifiers when present, and event summaries.
The default view hides large bodies; opening an item requests its structured
detail.

### Provenance

Every item has a visible provenance:

- **fact**: a required event accepted by the journal and included in replay;
- **diagnostic**: an optional, ignorable observation such as trajectory.

Availability is separate from provenance. A relation or detail that cannot be
proven from persisted identifiers is marked **unavailable** while retaining
the provenance of the item that refers to it.

Diagnostic absence never changes a work status, terminal outcome, message
relation, usage total, or any other authoritative conclusion.

### Loading and errors

- An empty session shows its identity and an empty-history explanation.
- A session with no root work can still show session, agent, transcript, and
  journal facts.
- A missing diagnostic record leaves the authoritative item present with a
  diagnostic-unavailable state.
- A missing or stale history projection is rebuilt from the journal before it
  is served; the TUI shows loading and never renders a knowingly mixed
  revision.
- An integrity failure is shown on the selected session and does not silently
  fall back to partial history.
- A paged query that becomes stale is restarted against the newer published
  revision rather than merging revisions.

## Acceptance criteria

- [x] Selecting a historical session for inspection does not open it or change
      the TUI's active session.
- [x] Work history is organized by root AgentInput and contains every related
      required input, processing, ModelStep, message, tool, action, interrupt,
      usage, report, and outcome fact in durable order.
- [x] A ModelStep displays its assistant message and ordered tool declarations
      from the required atomic relation, never from trajectory adjacency.
- [x] Work, Agents, Transcript, and Journal lenses link the same identities
      without introducing Turn, Run, or Execution IDs.
- [x] Journal commits and their event order are visible, and events committed
      in one revision are rendered as one atomic group.
- [x] Every item distinguishes required fact from optional diagnostic
      observation, and independently identifies unavailable detail or relation.
- [x] Removing all trajectory observations from a fixture does not change any
      authoritative work status or relation; only diagnostic detail disappears.
- [x] Large messages, prompt assemblies, tool payloads, and journal histories
      are not transferred until requested and lists remain cursor-paged.
- [x] A normal history query reads published read models and does not replay or
      scan journal segments; missing/stale models rebuild and then return the
      same result as full replay.
- [x] A query response joins only projections at one revision/checksum.
- [x] Older sessions without child-origin facts remain inspectable and label
      the exact origin unavailable instead of guessing.
- [x] TUI keyboard, pointer, narrow-layout, empty-state, and error-state tests
      pass together with package fmt, clippy, and tests.
- [x] After hostd restart, history overview/work/journal queries match the
      published snapshot without reading journal segments.
- [x] Cross-process soak covers child spawn/origin, approved write, compaction,
      and interrupt; usage correction and failed work survive aligned reads
      after the journal is hidden.
- [x] The loopback HTTP/SSE trajectory viewer, static assets, live fan-out,
      and `[trajectory]` bind/port/enabled settings are removed (ADR-029);
      leftover `[trajectory]` keys in user settings are ignored.
- [ ] Workspace-wide fmt/clippy/tests remain before accepting F-52 as complete.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What is the primary product concept? | Session History organized by Session, AgentInstance, root AgentInput, and ModelStep | These are the current durable grains; trajectory's Run model is obsolete. |
| What is authoritative? | Required journal facts | They are validated and replayed; trajectory is best-effort. |
| What role does trajectory keep? | Optional diagnostic enrichment | It uniquely carries prompt assembly, retries, fallbacks, and provider timing. |
| Is the UI a raw event viewer? | No; semantic lenses with an advanced Journal lens | Most questions are causal, while revision/event detail remains available for debugging. |
| Does inspecting a session resume it? | No | Read-only historical inspection must not change active product state. |
| Are reads implemented by journal replay? | No | F-37 requires write-time read models for query surfaces. |
| Does Session History replace the web viewer? | Yes (ADR-029) | TUI coverage landed; a second run-oriented live HTTP surface contradicts fact authority and explicit-refresh inspection. |
| Does it update live? | No; explicit refresh only | Historical comprehension does not require another realtime state path. |

## Fusion decisions (codex-rs)

This feature is a piko product decision and is not derived from a codex-rs
surface. codex-rs remains evidence for individual runtime behaviors already
accepted by F-31, F-36, F-48, and F-51; no viewer parity is required.

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Rollout/transcript inspection | kept (adapted) | Transcript is one lens over piko's journal-backed ancestry, not the authority for Agent work. |
| Runtime trace inspection | rejected as the history authority | Required journal facts provide the skeleton; optional diagnostics enrich it. |

## Open questions

1. None for this feature. Export and comparison remain later product
   decisions. Web-viewer retirement is ADR-029.

## Reference evidence

- [F-31 durable session journal](F-31-durable-session-journal.md)
- [F-36 agent run trajectory](F-36-agent-run-trajectory.md)
- [F-37 materialized read models](F-37-materialized-read-models.md)
- [F-48 authoritative agent lifecycle](F-48-authoritative-agent-lifecycle.md)
- [F-51 agent work lifecycle and control plane](F-51-agent-control-plane.md)
- [ADR-027 agent work lifecycle](../decisions/ADR-027-agent-work-lifecycle.md)
- [ADR-029 retire trajectory web viewer](../decisions/ADR-029-retire-trajectory-web-viewer.md)
