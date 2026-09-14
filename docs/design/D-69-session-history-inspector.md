# D-69: Session trajectory inspector (journal-derived)

> Status: redesign implemented 2026-09-13 (protocol/hostd/TUI landed;
> workspace fmt, clippy, and package tests pass; E2E assertions updated and
> pending an integration run-through)
> Implements: [F-52](../features/F-52-session-history-inspector.md)
> Decisions: [ADR-030](../decisions/ADR-030-shared-tui-split-pane.md),
> [ADR-028](../decisions/ADR-028-journal-derived-session-history.md),
> [ADR-029](../decisions/ADR-029-retire-trajectory-web-viewer.md),
> [ADR-027](../decisions/ADR-027-agent-work-lifecycle.md),
> [ADR-015](../decisions/ADR-015-host-owned-session-journal.md)

## Goal

Deliver a read-only TUI trajectory view that presents a historical session as
one flat, time-ordered stream per AgentInstance, with a lane strip for
step/tool activity and a tabbed detail pane. The redesign keeps the durable
history projection, aligned inspection bundle, revision discipline, and
child-origin capture from the previous design, and replaces the four-lens
surface (Work / Agents / Transcript / Journal) with a single trajectory view.

## What survives from the previous design (landed, unchanged)

- `session-store` history projection (`history.json`): ordered commits,
  causal indexes (`work_commit_indexes`, `agent_commit_indexes`,
  `message_to_step`, `tool_call_to_step`, `child_origins`, usage relations),
  fact/diagnostic provenance from `ignorable`.
- The aligned `InspectionBundle` query (`query_inspection`): head-checked,
  rebuild-once, typed busy/stale errors, no journal scan on the read path.
- Required `agent_origin_recorded_v1` capture and validation.
- Revision-bound cursors and `HistoryRevisionChanged` protocol result.
- Session selection without opening; explicit refresh only.

## What is removed

- **Work lens / work_page query / HistoryWorkPage DTO**: the stream spans
  works; per-root causal closure browsing is deleted.
- **Agents lens**: replaced by an agent selector row in the trajectory header
  (parent-indented list with lifecycle).
- **Transcript lens / transcript_page query / HistoryTranscriptPage DTO**:
  deleted; live Timeline covers conversation reading, and tree/branch
  inspection is a niche need cut for scope.
- **Journal lens / journal_page query / HistoryJournalPage DTO**: deleted;
  the journal stays inspectable on disk.
- **User-facing provenance filter** (`factsOnly` / `diagnostics` commands):
  deleted; fact/diagnostic split remains internal for detail enrichment.
- History tokens may drop kinds that only existed for removed lenses;
  `HistoryItemKind` stays a string so old tokens fail cleanly.

## Proposed design

### 1. Authority and projection layers (unchanged)

```text
events/*                      sole durable authority
   │ apply on append/rebuild
   ├── current.json           canonical entities and current/final values
   ├── history.json           ordered transitions + causal indexes
   └── trajectory.json        optional diagnostic content
             │
             v
       SessionHistoryQuery     revision-consistent join
             │ JSONL commands
             v
        TUI trajectory surface
```

### 2. Host application query (replaces the four operations)

`application::session_history::SessionHistoryQuery` keeps session resolution
through `SessionRepositoryPort` and the aligned bundle, and exposes:

```text
overview(session, cursor, limit)        -> agents + paged works + revision
agent_stream(session, agent, expected_revision, cursor, limit)
                                        -> flat ordered rows for one agent
lane_summary(session, agent, expected_revision)
                                        -> step/tool blocks for the strip
item_detail(session, revision, item_ref)
```

The host cache stores an `Arc<InspectionBundle>` per session/revision. Query
handlers borrow the shared snapshot, so opening one item never clones the full
current/history/trajectory bundle. The TUI keeps a revision-scoped cache of
details already opened during the inspection. It also caches formatted lines by
item, tab, width, and theme palette; viewport-only changes reuse those lines and
clone only the visible slice for painting.

History wheel gestures are safe to drain within the existing bounded input
cycle: they change only a viewport offset, so adjacent trackpad events are
applied before the cycle's single paint. List rendering projects only the
current visible range instead of cloning every loaded stream row while the
detail pane scrolls. The same visible range produces row hit rectangles using
absolute stream indexes, keeping pointer activation correct after scrolling in
both list-only and wide split layouts.

- `overview` keeps its shape (session identity, agents, revision) and remains
  the entry query. Work summaries are retained here only as counts/usage for
  the header; they are not a navigation level.
- `agent_stream` pages all durable items of one agent in journal order across
  works: user inputs, assistant messages (text/thinking), tool declarations
  and results as one row per tool call. Rows carry kind, preview, status,
  duration when known, relation IDs, and a `has_detail` flag. Diagnostic
  records never appear as stream rows; they only join detail/timing.
- `lane_summary` returns, for the selected agent: ordered ModelStep blocks
  (start, duration when the trajectory record exists, outcome, step index)
  and tool-call blocks (start, duration, status, call ID). Blocks reference
  the stream row's item token. Timing comes from trajectory diagnostics; a
  block without timing reports `timing: unavailable` and is drawn in sequence
  position. The summary is bounded (compact widths, capped block count) and
  paged by the same revision rules.
- `item_detail` keeps the current typed content union; Timing data joins the
  trajectory record by persisted identity (message/step/call IDs).

### 3. Protocol DTOs and commands

`protocol/src/session_history.rs` keeps `HistoryProvenance`,
`HistoryAvailability`, `HistoryItemRef`, `HistoryRelation`,
`HistoryItemKind`, `HistoryItemContent`, `HistoryItemDetail`, and
`HistoryAgentSummary`. Changes:

```rust
pub struct SessionHistoryOverview { ... }   // unchanged shape, entry query
pub struct HistoryStreamPage {              // replaces HistoryWorkPage
    pub session_id: String,
    pub agent_instance_id: String,
    pub revision: u64,
    pub items: Vec<HistoryItemSummary>,
    pub next_cursor: Option<String>,
}
pub struct HistoryLaneBlock {
    pub kind: LaneBlockKind,            // ModelStep | ToolCall
    pub reference: HistoryItemRef,      // stream row token
    pub label: String,
    pub status: String,                 // outcome/status summary
    pub sequence: u32,                  // journal-order position
    pub started_at: Option<i64>,        // diagnostic; None = unavailable
    pub duration_ms: Option<u64>,       // diagnostic; None = unavailable
}
pub struct HistoryLaneSummary {
    pub session_id: String,
    pub agent_instance_id: String,
    pub revision: u64,
    pub blocks: Vec<HistoryLaneBlock>,
    pub timing_available: bool,         // false = sequence strip mode
}

pub enum LaneBlockKind { ModelStep, ToolCall }
```

Commands and results:

```text
SessionHistoryOverviewGet     -> SessionHistoryOverviewGot   (kept)
SessionHistoryAgentStreamGet  -> SessionHistoryAgentStreamPaged  (new)
SessionHistoryLaneGet         -> SessionHistoryLaneGot           (new)
SessionHistoryItemGet         -> SessionHistoryItemGot           (kept)
```

Removed: `SessionHistoryWorkPageGet`, `SessionHistoryJournalPageGet`,
`SessionHistoryTranscriptPageGet` with their result variants, plus the
`HistoryFactsOnly` / `HistoryDiagnostics` command surface. Cursor format and
revision binding rules are unchanged.

### 4. TUI surface

`SurfaceId::History` stays the internal `CoverBody` surface identifier and is
opened by the user command `/trajectory`. The internal identifier and
`SessionHistory*` protocol DTO names remain implementation details. The panel
is rebuilt around one view:

```rust
pub struct HistoryPanel {
    inspected_session_id: Option<String>,
    revision: Option<u64>,
    agent_id: Option<String>,           // selected agent stream
    agents: Vec<HistoryAgentSummary>,   // selector rows
    stream: PagedList<HistoryItemSummary>,
    lanes: Option<HistoryLaneSummary>,
    detail: LoadState<HistoryItemDetail>,
    active_tab: DetailTab,              // Summary | Payload | Result | Timing
    filter: String,
    selection: ...,
}
```

Layout (wide):

```text
┌────────────────────────────────────────────────────┐
│ session · agent selector · rev N        [refresh]  │
│ Model ▏▁▁▂▁▃▁▁▂▁ lane (blocks by time)             │
│ Tools ▏▂▁▁▂▁▁ lane (shares the same x time axis)   │
├──────────────────────────────┬─────────────────────┤
│ stream rows (role badges)    │ detail tabs         │
│ USER    Build a tool that…   │ Summary|Payload|    │
│ ASSIST  I'll start by…       │        Result|Timing│
│ TOOL    bash  "pwd && …" 21ms│ …                   │
└──────────────────────────────┴─────────────────────┘
```

Lanes stack vertically and share one time axis (`solve_lane_strip` in
`tui-layout`): a block's x position is the same in every lane, so a ModelStep
aligns with the tool calls it produced. Block width maps to recorded duration
when every segment has timing, otherwise the whole strip degrades to
sequence mode and labels itself as such. A dedicated one-row divider spans the
surface content below the lanes; it is layout-owned so painting and pointer
geometry reserve the same row.

Narrow layout: the same state as drill-down pages (stream → detail), lane
strip collapses to a single compact row. The shared Split Pane component owns
wide/compact composition, insets, and paint/hit geometry, as in the previous
refinement.

Bindings:

```text
j/k or arrows      stream selection
Enter              open detail (fetches body only now)
Tab                move focus stream ⇄ lane ⇄ detail ⇄ agent selector
1..4 or t          detail tab cycling
/                  local filter over loaded rows
s                  agent selector
r                  explicit refresh
Esc                close detail, then close surface
```

- Selecting a lane block selects and reveals its stream row; selecting a
  message row highlights step blocks; selecting a tool row highlights its
  tool block.
- ModelStep stream items use a one-line ending-marker presenter while retaining
  their durable journal position and selectable detail identity. They are not
  assigned the badge-column grammar used by messages and tool calls.
- Wheel and trackpad input over the stream moves its viewport by multiple rows
  per event without requesting detail. Selection is clamped to the nearest
  visible edge only after it leaves the viewport. Keyboard selection continues
  to drive the master-detail preview.
- Detail tabs map to typed content: Summary (identities, status, commit
  meta), Payload (message content / input / tool arguments), Result (tool
  result or error), Timing (start/finish/duration, retries, fallback,
  provider/model — diagnostic; absent shows "diagnostic unavailable").
- Summary uses separate content, relation, journal, and snapshot sections.
  Long identifiers use aligned hanging wraps rather than flowing beneath their
  field labels.
- Historical tool-call details adapt the persisted message and joined
  trajectory observation into an expanded, read-only `ToolEntry`, then call the
  Timeline `tool_lines` renderer. This reuses every per-tool presenter without
  importing Timeline viewport, hover, folding, or hit-state ownership into the
  History surface.
- Agent switching clears stream pages and detail, keeps session and revision.
- The session chooser keeps `SessionList` and never sends `SessionOpen`.

### 5. Trajectory integration

Unchanged join rules: assembly → root work, model-step record → step, tool
record → call ID, by persisted identity only; no timestamp matching. With the
Journal lens gone, unjoined diagnostics are simply not shown; the timing tab
reports absence. This removes the previous "unjoined diagnostics remain
Journal-only" presentation obligation.

## Package impact

| Package | Change |
|---|---|
| `piko-session-store` | No projection change; only query consumers shrink |
| `piko-protocol` | Add stream/lane DTOs and commands; remove work/journal/transcript DTOs and commands |
| `piko-hostd` | Replace `work_page`/`journal_page`/`transcript_page` with `agent_stream`/`lane_summary`; detail mapping keeps typed content |
| `piko-tui` | Rebuild `features/history/` around the single trajectory view; delete lens/present machinery |

## Failure and cancellation

Unchanged from the previous design: read-model rebuild before serve, bounded
bundle retries with typed stale/busy errors, revision-change restarts the
page, detail errors never erase the stream, missing current entity is an
integrity error, missing diagnostic is labeled unavailable.

## Verification

- Protocol serde tests for the new stream/lane commands/results, cursor
  binding, and removed-command rejection.
- hostd tests: agent stream ordering across works, per-agent isolation
  (child events never leak into the parent stream), lane summary timing
  present/absent, lazy detail, revision change.
- TUI tests: agent selector switching, stream paging, lane↔stream selection
  linkage, tab content mapping, absent-timing labeling, narrow/wide
  equivalence, refresh, active-session isolation.
- Keep existing store/bundle/rebuild tests (projection unchanged).
- Cross-process E2E: reuse the history soak but assert the stream view after
  restart instead of the removed lenses.
- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `cargo test --workspace`.

## Rollout

1. Protocol: add stream/lane commands + DTOs, remove the retired ones.
2. hostd: `agent_stream` + `lane_summary`, delete retired queries.
3. TUI: rebuild the panel; delete lens/present/provenance-filter code.
4. Update E2E/soak assertions from lenses to the stream view.
5. Visual acceptance on the single view (wide/narrow, long content, CJK,
   absent diagnostics), then close F-52.

## Alternatives considered

- **Keep Transcript as a secondary lens:** rejected for scope; the live
  Timeline covers reading, and tree/branch inspection was niche.
- **Keep Journal behind an "advanced" toggle:** rejected; an in-product
  commit browser is store debugging, and the journal file stays accessible.
- **Interleave all agents in one stream:** rejected by product decision;
  unrelated agent chatter interleaved makes root-agent review noisy.
- **Live-follow the active session:** rejected by product decision; explicit
  refresh keeps this a pure historical surface over published snapshots.
