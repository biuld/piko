# D-69: Journal-derived session history inspector

> Status: in progress
> Implements: [F-52](../features/F-52-session-history-inspector.md)
> Decisions: [ADR-030](../decisions/ADR-030-shared-tui-split-pane.md),
> [ADR-028](../decisions/ADR-028-journal-derived-session-history.md),
> [ADR-029](../decisions/ADR-029-retire-trajectory-web-viewer.md),
> [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md),
> [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Deliver a read-only TUI Session History surface backed by a new durable
history projection. The surface explains historical sessions through the
current Session/AgentInstance/root AgentInput/ModelStep model, preserves
journal commit boundaries, and joins optional trajectory diagnostics without
making them authoritative or requiring realtime updates.

## Implementation status

The current implementation includes the durable history projection and aligned
inspection bundle, required child-origin capture and validation, host overview,
work, journal, and transcript queries, lazy detail, and the TUI Work, Agents,
Transcript, and Journal browsing path. Work pages keep required facts as peers
and attach matching trajectory observations as diagnostic children by persisted
identity; unjoined diagnostics remain Journal-only. Transcript pages walk the
session tree and agent-private message ancestry independently from causal work
order. History tokens carry the published snapshot revision separately from the
event position encoded in the token. Cursors bind the revision and query scope;
revision drift produces a structured `HistoryRevisionChanged` result.

The aligned inspection path reads identity, head, and projections without
accessing journal segments. It retries publication races, repairs once if
necessary, and returns a typed store busy error if alignment still fails.
TUI queries have panel-owned command correlation, including the reused
`SessionList` operation, so replies after close or session replacement cannot
open another surface. Work, Journal, and Transcript fetch subsequent pages near
the end of the loaded list. `/` starts a local filter, `a` shows facts only,
`d` toggles diagnostic visibility, `s` selects another session, and `r`
explicitly refreshes. Wide terminals keep selector and detail side by side
without changing selection authority; pointer hits cover lens tabs and rows.

The F-36 loopback HTTP/SSE trajectory viewer is retired (ADR-029). Workspace-wide fmt, clippy, and tests passed on 2026-09-06. F-52 remains
in progress pending complete UI visual acceptance.

### UI refinement status (2026-09-05)

The refinement is implemented in `features/history/` and the shared
`ui::components::split_pane` component. History uses independent selector/detail
viewports, explicit summary inspection (`i` and row information targets),
separate detail loading/errors, and a shared preparation recipe for paint and
pointer geometry. Lens navigation occupies its own row; revision remains in
surface chrome at narrow widths.

Typed detail distinguishes thinking, text, image placeholders, tool arguments
and results, and exposes all loaded prompt blocks rather than the former
eight-label preview. Full available identities and commit metadata remain
scrollable. Provenance and unavailability have independent textual labels.

The 2026-09-06 follow-up keeps full lens names at 40 columns where they fit,
shows matched/loaded row counts within the current provenance scope, and wraps
detail feedback with its opened-item context. Summary-only rows no longer
advertise a body request. Detail viewport rendering lives in `history/detail.rs`.

See [verification evidence](../verification/F-52-history-ui-refinement.md).
The inspection authority and explicit-refresh model remain unchanged.

## Constraints and non-goals

- hostd remains authoritative for user-visible state and query composition.
- The journal remains the sole durable authority. History is disposable and
  rebuildable.
- Ordinary queries read aligned write-time projections and never replay or
  scan journal segments (F-37).
- Do not add Turn, Run, or Execution identities, aliases, or compatibility
  adapters. The work key is `root_input_id`.
- TUI does not read `~/.piko`, session files, or the trajectory HTTP API.
- Query responses are bounded. Large bodies are fetched by stable content
  reference only after expansion.
- No SSE, polling, live follow, automatic refresh, or model invocation.
- The loopback HTTP/SSE trajectory viewer, static assets, and `[trajectory]`
  bind/port/enabled settings are removed (ADR-029).
- Desktop is out of scope. A later desktop surface may consume the protocol
  DTOs but does not constrain this TUI design.

## Proposed design

### 1. Authority and projection layers

The inspector composes three published views at one journal watermark:

```text
events/*                      sole durable authority
   │ apply on append/rebuild
   ├── current.json           canonical entities and current/final values
   ├── history.json           ordered transitions + causal indexes (new)
   └── trajectory.json        optional diagnostic content
             │
             v
      SessionHistoryQuery     revision-consistent join
             │ JSONL commands
             v
       TUI History surface
```

`current.json` continues to own message bodies, ModelStep relations, AgentInput
entities, effective accounting, agent identity, transcript ancestry, tree
entries, compactions, and reports. `trajectory.json` continues to own prompt
assembly and best-effort runtime observations.

`history.json` retains information that a current-state aggregate necessarily
collapses or removes: revision/event order, disposition transitions, resolved
pending actions, interrupt history, agent lifecycle changes, branch-selection
history, usage corrections, and the commit groups that contained them. It
stores stable references and bounded summaries, not duplicate message,
prompt, image, tool-result, or arbitrary JSON bodies.

### 2. Session-store history projection

Add `readmodels/history.rs` with an internal, serializable projection:

```rust
pub struct HistoryProjection {
    pub revision: u64,
    pub commits: Vec<HistoryCommit>,
    pub work_commit_indexes: BTreeMap<String, Vec<usize>>,
    pub agent_commit_indexes: BTreeMap<String, Vec<usize>>,
    pub message_to_step: BTreeMap<String, String>,
    pub tool_call_to_step: BTreeMap<String, String>,
    pub child_origins: BTreeMap<String, AgentOriginRef>,
}

pub struct HistoryCommit {
    pub revision: u64,
    pub commit_id: String,
    pub committed_at: i64,
    pub producer: String,
    pub causation_id: Option<String>,
    pub correlation_id: Option<String>,
    pub events: Vec<HistoryEvent>,
}

pub struct HistoryEvent {
    pub event_id: String,
    pub kind: String,
    pub provenance: HistoryProvenance,
    pub agent_instance_id: Option<String>,
    pub root_input_id: Option<String>,
    pub model_step_id: Option<String>,
    pub entity_id: Option<String>,
    pub transition: Option<HistoryTransition>,
    pub summary: String,
}
```

`HistoryTransition` is a typed internal enum for data that is lost from the
current aggregate: input disposition changes, pending-action request/resolve,
interrupt request, agent lifecycle, branch selection, usage correction, inbox
consumption, and terminal work outcome. Events whose complete entity remains
in `current.json` retain only its stable ID and presentation summary.

Required events use `HistoryProvenance::Fact`. Recognized optional
`trajectory.*` events use `Diagnostic`; other optional events retain their
event type and identity when it can be extracted, with a generic diagnostic
summary. An unrecognized optional event never changes causal indexes.

The reducer receives the entire `DurableCommit`, preserving revision and event
array order. It updates indexes only from persisted IDs; no timestamp or
adjacency inference is allowed.

The history file uses the same envelope as other read models:

```text
schemaVersion, sessionId, journalGeneration,
throughRevision, throughChecksum, projection
```

Publication becomes:

1. apply commit to aggregate, trajectory, and history projections;
2. atomically replace catalog, current, trajectory, and history files;
3. atomically replace `head.json` last;
4. only then publish the commit to query consumers.

Create, fork, import, append, idempotent republish, and rebuild update all four
models. Missing, stale, corrupt, or version-incompatible history files trigger
the existing full rebuild path. Fork/import rebuild destination history from
destination commits and never copy a source projection.

### 3. Revision-consistent inspection bundle

Do not call `query_current`, `query_history`, and `query_trajectory`
independently: an append between calls could mix revisions. Add a
session-store query that loads one published watermark and returns:

```rust
pub struct InspectionBundle {
    pub revision: u64,
    pub checksum: String,
    pub current: SessionAggregate,
    pub history: HistoryProjection,
    pub trajectory: TrajectoryProjection,
}
```

`query_inspection(path)` performs this loop:

1. read identity and `head.json`;
2. load current/history/trajectory whose envelopes match that head;
3. re-read `head.json` and accept only when unchanged;
4. on missing/stale data, open once, rebuild all projections, and retry;
5. bound concurrent-head retries, returning a typed busy/stale error rather
   than a mixed bundle.

The bundle is internal to session-store/hostd and is not serialized directly
to clients.

### 4. Exact child-agent causation

Agent identity already persists parent AgentInstance but not the spawning work,
ModelStep, or tool call. Add a required `agent_origin_recorded_v1` fact:

```rust
pub struct AgentOriginRecordedV1 {
    pub child_agent_instance_id: String,
    pub parent_agent_instance_id: String,
    pub parent_root_input_id: String,
    pub origin_model_step_id: String,
    pub origin_tool_call_id: String,
    pub recorded_at: i64,
}
```

When a model tool creates a child, hostd commits `agent_created` and
`agent_origin_recorded_v1` atomically. Manual/system creation without a model
tool emits only `agent_created`. The aggregate validates that the parent root,
step, and tool declaration exist and agree. Old sessions remain readable and
show exact origin as unavailable.

Tool result linkage needs no new fact: `Message::ToolResult` carries the tool
call ID, and `ModelStepCommitted.tool_call_message_ids` identifies the ordered
declarations. The history reducer builds `tool_call_id -> model_step_id` only
after validating those required relations.

### 5. Host application query

Add `application::session_history::SessionHistoryQuery`. It resolves unopened
sessions through `SessionRepositoryPort`, obtains one `InspectionBundle`, and
maps internal store types to protocol DTOs. It never attaches a session,
invokes a model, or changes the active session.

The query exposes four operations:

```text
overview(session, cursor, limit)
work_page(session, root_input_id, cursor, limit)
journal_page(session, cursor, limit, provenance_filter)
item_detail(session, revision, item_ref)
```

- `overview` returns session identity, agents, paged work summaries, counts,
  and the published revision.
- `work_page` returns lightweight ordered items for one root. Related facts
  come from history/current; matching trajectory records attach as diagnostic
  children rather than peer authoritative events.
- `journal_page` pages whole commits, never splitting the events of one commit.
- `item_detail` resolves one stable reference to a full message, tool payload,
  prompt assembly/block, usage fact, report, or structured event detail.

Every paged request and cursor carries the published revision. A request made
against an older revision returns `HistoryRevisionChanged { current_revision }`;
the TUI restarts the page rather than merging snapshots.

### 6. Protocol DTOs and commands

Add `protocol/src/session_history.rs`. Client DTOs are semantic and do not
expose `SessionAggregate`, `RawEvent`, filesystem paths, or read-model formats.

Core shapes:

```rust
pub enum HistoryProvenance { Fact, Diagnostic }
pub enum HistoryAvailability { Available, Unavailable { reason: String } }

pub struct SessionHistoryOverview {
    pub session_id: String,
    pub revision: u64,
    pub agents: Vec<HistoryAgentSummary>,
    pub works: Vec<HistoryWorkSummary>,
    pub next_cursor: Option<String>,
}

pub struct HistoryWorkSummary {
    pub root_input_id: String,
    pub agent_instance_id: String,
    pub origin: AgentInputOrigin,
    pub input_preview: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub outcome: Option<AgentWorkProcessingStatus>,
    pub step_count: u32,
    pub tool_count: u32,
    pub message_count: u32,
    pub usage: Option<Usage>,
}

pub struct HistoryItemSummary {
    pub item_ref: HistoryItemRef,
    pub revision: u64,
    pub event_index: u32,
    pub kind: HistoryItemKind,
    pub provenance: HistoryProvenance,
    pub availability: HistoryAvailability,
    pub relation: HistoryRelation,
    pub summary: String,
    pub has_detail: bool,
}
```

`HistoryRelation` carries optional agent/root/step/message/tool/input IDs.
Availability is separate so a fact with a missing legacy relation does not
become a diagnostic item.
`HistoryItemRef` is an opaque, bounded host-issued token plus revision; clients
do not construct storage keys. Detail responses are typed by content kind and
have an explicit unavailable variant. Unknown future item kinds render through
a generic `Other { name }` form.

Commands and results:

```text
SessionHistoryOverviewGet  -> SessionHistoryOverviewGot
SessionHistoryWorkPageGet  -> SessionHistoryWorkPaged
SessionHistoryJournalPageGet -> SessionHistoryJournalPaged
SessionHistoryItemGet      -> SessionHistoryItemGot
```

All commands carry `command_id`, `session_id`, cursor/limit where applicable,
and `expected_revision` after the first response. Reuse `SessionList` for the
session chooser.

### 7. TUI state and navigation

Add `SurfaceId::History` as `CoverBody`, with a feature-owned input profile and
guidance. `/history` opens it. The state is independent from the active chat:

```rust
pub struct HistoryPanel {
    inspected_session_id: Option<String>,
    revision: Option<u64>,
    lens: HistoryLens,
    breadcrumb: Vec<HistoryLocation>,
    overview: LoadState<SessionHistoryOverview>,
    works: PagedList<HistoryWorkSummary>,
    items: PagedList<HistoryItemSummary>,
    detail: LoadState<HistoryItemDetail>,
    selection: HistorySelection,
    filters: HistoryFilters,
}
```

Lenses are Work, Agents, Transcript, and Journal. Work is the default. One
panel owns sub-navigation so the focus stack receives a single surface; moving
between internal levels does not open nested global modals.

Wide terminals render selector/detail panes. Narrow terminals render the same
state as drill-down pages. The breakpoint changes layout only, never selection
or authority. Lists use the shared viewport/scroll primitives and fetch the
next cursor near their end. Full content is formatted only after
`SessionHistoryItemGot`.

Bindings:

```text
Tab / Shift+Tab    next/previous lens
j/k or arrows      selection
Enter              enter/expand/fetch detail
i                  inspect the loaded summary without fetching a body
Backspace/Left     one breadcrumb level back
/                  filter
a                  facts only
d                  diagnostic visibility
r                  explicit refresh
Esc                close filter, go back, then close surface
```

The Session selector uses existing `SessionList` data but a selection sets
`inspected_session_id`; it must never send `SessionOpen`. Closing the surface
discards panel-local pages and leaves the active session, editor, timeline,
agent selection, and queues untouched.

### 8. Lens projections

The TUI performs presentation-only grouping over host DTOs. Rows are scan
lines: typed kind or status on the left, counts/clocks on the right. Detail
is a typed body (message text, usage, assembly summary), not Debug enums or
pretty-printed protocol JSON. Diagnostic children stay muted and labeled
`diag`. Empty and loading copy is per-lens, not a generic dump.

- **Work** groups a root input, related input transitions, ModelSteps,
  committed messages, tool declarations/results, controls, usage, reports,
  outcome, and attached diagnostic children.
- **Agents** nests agents by persisted parent identity and groups their work.
  Exact spawn links use `agent_origin_recorded_v1`; missing legacy origin is
  visibly unavailable.
- **Transcript** renders message ancestry and tree entries independently from
  work order, with links back to root and step.
- **Journal** renders commit cards in revision order and keeps all events of a
  commit together.

Host DTO order is authoritative. TUI must not sort causal items by timestamp,
derive terminal state from the last visible row, aggregate usage, or infer
child/tool relations.

### 8a. Information presentation refinement

Keep all authority, revision, paging, and explicit-refresh constraints above.
The inspector remains a causal history browser; Timeline message-card spacing
and live-follow behavior do not define this surface.

Feature-owned presenters separate context, scan rows, and structured detail
sections in `features/history/present/`. They consume semantic DTOs and retain
stable identity independently of displayed text. Full identities and relations
remain available in detail; TUI must not reconstruct them from shortened IDs
or opaque item tokens. If required evidence is absent from a DTO, extend the
host query and protocol deliberately rather than reading files or inferring it.

| Lens | Primary scan information | Detail emphasis |
|---|---|---|
| Work | Input preview, target agent, origin, recorded outcome | Input transitions, ordered ModelSteps and their persisted message/tool relations, controls, reports, and usage corrections |
| Agents | Agent identity, parent hierarchy, lifecycle, work count | Exact spawn/caller relations, agent work, and inbox reports; unavailable legacy origin stays explicit |
| Transcript | Message/entry kind, content preview, ancestry and branch markers | Structured content, selected/off-branch context, and persisted links to root and step |
| Journal | Revision, producer, commit boundary, ordered event summaries | Full commit identity, time, causation/correlation, and individual fact/diagnostic evidence |

Keep authoritative host order within each lens. Group labels and indentation
may clarify persisted relations but must not move facts across commits or
reorder causal facts to manufacture a tree. Diagnostic children remain labeled
and subordinate; provenance and availability are separate presentation fields.
Work outcome is the host-reported outcome, never an inference from a tool exit
or the last displayed message. Usage displays host-computed values without
client aggregation.

Replace opaque count suffixes with compact understandable labels. Establish
width priorities in the row model: identity/summary, critical state, and
provenance survive before optional clocks/counts/usage. Revision remains
accessible for every fact; Journal retains revision as primary context. Use
shared column helpers and bounded indentation so deep ancestry and CJK labels
do not consume the whole summary column.

Detail presenters produce sections rather than flattening every body into a
string: semantic heading and provenance, primary content, then relations and
technical evidence. Reuse product-independent text/code/diff primitives where
appropriate without importing Timeline's projection or state. Distinguish
text/thinking/non-text blocks and tool input/result. Opening a prompt assembly exposes all its loaded blocks and their metadata
in the scrollable detail pane.
Unknown kinds retain a labeled generic fallback. Every display limit reports
omission; scrolling or explicit bounded detail requests expose remaining
available content. Do not add eager full-body loading or unbounded responses.

### 8b. Layout and interaction refinement

Compose one prepared History frame from outer `Pane` chrome and the shared
[Split Pane component](../../packages/tui/docs/design/split-pane.md), specified
by its [PRD](../../packages/tui/docs/features/split-pane.md). Split Pane owns
wide/compact composition, content insets, separator, pane feedback, and shared
paint/hit geometry. It reuses the existing `piko-tui-layout::DividerSplit`;
History must not retain its three private copies of split calculations. Lens
IDs, breadcrumbs, selected/opened identities, focus actions, requests, and
independent content viewports remain in History. Give both
panes explicit content insets and consistent section spacing; avoid nested
boxes or per-row blank padding. Exact cell counts and minimum pane widths are
implementation choices to validate visually, not new configuration settings.
The shared component derives compact fallback from those width constraints.

Maintain separate list and detail viewport state. Track selected row identity,
opened detail identity, detail revision, and panel-local focus explicitly.
Selection changes do not request full content. Enter/open requests detail and
identifies its owner in the detail heading; detail cannot appear to belong to
another selected row. Back restores the previous list position. Resize changes
placement without resetting identities, breadcrumb, or fetching content.

Tab/Shift+Tab continue to switch lenses. The feature input profile exposes `i` for summary inspection and open/back
actions with their effective bindings in guidance;
opening detail focuses its scrollable content, and back returns to the list.
Wheel routing uses the pane under the pointer. Keyboard scrolling targets the
focused pane. Selection remains distinct from hover and pane focus. Text
selection, where supported by shared primitives, must not activate navigation.

Separate overview/page/detail request states and keep existing command
correlation. Show local filter scope, loaded/matched counts, and whether more
pages exist; do not report a loaded-page count as a whole-session total.
Pagination progress and errors must not replace completed rows. Detail errors
stay within detail with retry guidance. On revision change invalidate all
revision-bound detail/pages before accepting the new snapshot; preserve stable
navigation identity only if it can be resolved again at that revision.

The implementation separates semantic presenters, the shared Split Pane
component with independent consumer viewports, and navigation/state feedback. Record
visual evidence for each lens at narrow/wide sizes with long content, CJK,
large histories, unknown kinds, unavailable diagnostics, and legacy relations.
Update F-52's new unchecked criteria only after implementation and verification.

### 9. Trajectory integration and retired web viewer

Known trajectory observations attach by persisted root, step, message, and tool
call identities. Prompt assembly attaches to root work; model-step diagnostics
attach to ModelStep; tool transitions attach to the declared tool call.

When an observation cannot be joined by stable identity, it remains visible
only in the Journal lens as an unjoined diagnostic. No nearest-time matching is
allowed. Historical dropped-record counters are advisory because process-local
drop counts do not survive restart; the UI describes only observed absence.

ADR-029 retires the loopback HTTP/SSE viewer, its static assets, live fan-out,
and `[trajectory]` bind/port/enabled settings. Leftover `[trajectory]` keys in
user settings.toml are ignored. Capture, `trajectory.json`, and this diagnostic
join remain.

## Package impact

| Package | Change |
|---|---|
| `piko-session-store` | History reducer/file, aligned inspection bundle query, child-origin validation, publish/rebuild integration |
| `piko-protocol` | History DTOs and JSONL commands/results; required child-origin fact DTO |
| `piko-hostd` | SessionHistoryQuery, command dispatch, unopened-session resolution, bounded detail mapping |
| `piko-orchd-api` | Carry stable parent root/step/tool causation on child creation commit request |
| `piko-orchd` | Populate exact child origin from the multi-agent tool invocation |
| `piko-tui` | `/history`, History cover-body surface, paging, lenses, detail rendering, filters and bindings |

## Reusable infrastructure

No `island-rs` change required. This feature is TUI-only. A future desktop
inspector can consume the host protocol but should use Island primitives for
its product-independent presentation.

## Failure and cancellation

- History inspection has no mutation or model cancellation path. Escape only
  changes local navigation or closes the surface.
- If a read model is absent, stale, corrupt, or incompatible, session-store
  rebuilds all aligned projections before serving the query.
- If the published head changes while assembling a bundle, query retries up to
  a fixed bound and returns a typed stale/busy response instead of mixed data.
- A stale page cursor causes a revision-changed response; TUI clears only the
  inspected pages and refreshes from the first page.
- A missing current entity referenced by a history item is an integrity error,
  not an empty detail.
- A missing optional trajectory entity returns diagnostic unavailable while
  leaving authoritative detail intact.
- Invalid or unverifiable child origin rejects the creating commit. Legacy
  agents without the new fact remain valid and render unknown origin.
- TUI transport failure preserves the previous complete page with an error
  notice; it never merges a partial response into the page.

## Verification

- Protocol serde tests for every command/result, provenance, cursor, unknown
  item fallback, and unavailable detail.
- Session-store reducer tests for revision/event order, disposition history,
  resolved actions, interrupts, lifecycle, branch changes, usage corrections,
  optional observations, and causal indexes.
- Property tests: history projection from incremental append equals full
  replay; removing optional events leaves every fact/index result unchanged.
- Publication tests: create/append/fork/import/rebuild align catalog, current,
  trajectory, history, and head; corruption of any file rebuilds all.
- Concurrency test: an append between bundle file reads never produces a mixed
  revision response.
- Child-origin tests for valid atomic creation, mismatched parent/root/step/tool
  rejection, manual creation, and legacy history.
- hostd tests for unopened-session queries, paging, revision changes, lazy
  detail, missing diagnostics, integrity errors, and no session attach/model
  invocation.
- UI refinement tests for independent list/detail scroll, opened-item identity,
  return/resize restoration, prepared pointer geometry, narrow-width priorities,
  independent provenance/availability, explicit preview omissions, and separate
  page/detail/filter/error feedback. Visual fixtures cover all four lenses.
- TUI tests for active-session isolation, four-lens navigation, wide/narrow
  layout equivalence, paging, filters, fact/diagnostic styling, unavailable
  origin, refresh, pointer hit regions, and transport errors.
- Cross-process E2E: create a multi-agent session with steer, tool work,
  approval, usage correction, compaction, and failure; restart hostd and obtain
  the same history without journal replay on the aligned path.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace`.

## Alternatives considered

- **Port the trajectory web viewer into TUI:** rejected because it promotes
  best-effort observations and the obsolete Run model over required facts.
- **Read raw journal segments on demand:** rejected by F-37; ordinary product
  queries require published read models and bounded latency.
- **Build history only from `current.json`:** rejected for the complete
  feature because current state discards resolved actions, prior lifecycle and
  disposition transitions, branch-selection history, and exact commit order.
- **Copy full event payloads into `history.json`:** rejected because it
  duplicates messages, images, prompts, and tool results. Store references and
  transition data; fetch bodies from aligned projections.
- **Merge facts and observations by timestamp:** rejected because timestamps
  do not establish causal identity or within-commit atomicity.
- **Make Journal the only lens:** rejected because raw event vocabulary is a
  poor primary explanation of work, agents, and transcript structure.
- **Keep the web viewer after TUI coverage:** rejected by ADR-029. A second
  run-oriented live HTTP surface reintroduces an obsolete inspection model.

## Rollout

1. Land F-52, ADR-028, and D-69; reconcile F-36/F-37 indexes and authority
   language.
2. Add the history reducer/file and publish/rebuild/alignment tests, without a
   client surface.
3. Add the revision-consistent inspection bundle, protocol DTOs, host query,
   overview/work paging, and lazy item detail.
4. Add the TUI `/history` shell, historical session selection, Work lens, and
   narrow/wide navigation.
5. Add Agents and Transcript lenses, then exact `agent_origin_recorded_v1`
   capture and validation. (landed)
6. Add the Journal lens, trajectory diagnostic enrichment, provenance filters,
   and large-session performance verification. (landed: Journal, diagnostic
   children, filters, cursor-paged large overviews, and lazy bodies)
7. Retire the web viewer, SSE path, and `[trajectory]` settings (ADR-029).
   Restart without journal replay and the multi-agent soak (spawn, approved
   write, compaction, interrupt; usage correction and failed work on aligned
   reads) are covered. Workspace-wide fmt/clippy/tests passed on 2026-09-06;
   complete UI visual acceptance remains before accepting F-52.

8. Refine information presentation and interaction per sections 8a/8b and the
   F-52 UI acceptance criteria. Core refinement is implemented; partial rendered
   buffer and regression evidence is recorded separately from the prior
   query/projection verification. Full interactive visual acceptance remains open.
