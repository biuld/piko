# F-36: Agent run trajectory

> Status: implemented (F-36/D-49, V-49)
> Priority: P1
> Source evidence: piko product decision; gap analysis over F-15 slices
> (D-30 prompt debugging, D-15/D-46 OTel inspection) and the F-31 durable
> journal

## Summary

piko records the complete trajectory of every agent execution as a durable,
host-owned, locally queryable step graph: the assembled prompt, every model
step with its full request and response (including thinking), every tool
batch with arguments, status, results, errors, and timing, links to child
agent runs, retries and fallbacks, and the terminal outcome. The trajectory is
the single source of truth for "what the agent did and why". It replaces the
process-local latest-only prompt-debug capture (D-30) and the OTel span
export, while OTel metrics and unified logs remain as projections over the
same runtime events.

## Problem

Today no single record answers "what did this run actually do?":

- The session journal (F-31) durably preserves committed facts — messages
  (including thinking and tool call/result content), executions with prompt
  digests, per-step usage, compaction, and world state — but it deliberately
  omits the assembled prompt body, per-step timing, retries/fallbacks, and
  step-level causality beyond committed messages.
- Prompt debugging (D-30) captures the exact assembled prompt and llmd model
  inputs with full content, but it is process-local and latest-only: hostd
  restart clears it, and only one snapshot per session/agent survives.
- OTel spans (D-15/D-46) provide the causal shape and timing, but the assembly
  span records metadata only, GenAI content attributes are opt-in, redacted,
  and truncated at hard size caps, and inspection requires an external
  backend that piko does not ship or query.
- Rollout paging (D-31) is a projection of committed messages, not a step
  record.

The developer iterating on piko needs a durable, content-complete step record
that survives restarts, works offline without an external backend, and can be
replayed or inspected locally. The trajectory closes that gap by consolidating
the causal, content, and timing planes into one host-owned record, making
prompt debugging and OTel span export redundant.

## User journeys

1. A developer runs a turn, hostd restarts, and the turn later fails. After
   restart, the failed run's full step graph — assembly, model requests and
   responses, tool calls and results, retries, child runs — is available
   locally, without re-running anything or connecting to an external backend.
2. A regression appears after a prompt or tool change. The developer compares
   two runs of the same agent and locates the first divergent step.
3. A multi-agent turn misbehaves. The developer opens the parent trajectory,
   follows a child-run link, and inspects the child's own model and tool
   steps.
4. While a turn runs, the developer opens the latest trajectory in the web
   viewer and follows steps as they commit.

## In scope

- Durable per-execution trajectory recording with the step graph: assembly,
  model steps, tool batches, child-run links, retries/fallbacks, and terminal
  outcome.
- Durable records in the session journal as observational event types; the
  journal remains the single durable record, with acknowledged facts and
  observational records in one stream.
- Best-effort, side-effect-free capture: trajectory writes never block, fail,
  or alter agent execution.
- Crash/restart semantics: partial trajectories are preserved; restart never
  clears them.
- Content policy: full content is always captured with no truncation and no
  disable switch; record size is naturally bounded by existing runtime limits
  (context window, max output tokens, tool-output caps).
- Read-only query path: list runs, fetch one run, page large trajectories
  (HTTP endpoints for the web viewer); observational — no session mutation and
  no model invocation.
- A read-only web viewer served by hostd over loopback HTTP: structured
  step-graph browsing that replaces the prompt-debug surface.
- Retirement of D-30 once the trajectory query path lands; removal of the
  OTel span exporter and D-46 GenAI content attributes; OTel metrics and
  unified logs retained.

## Out of scope

- Trajectory and assembly records never alter the acknowledged session-state
  projection (they are observational event types, durable and replayable but
  not authoritative facts).
- Streaming deltas: transient realtime output stays transient per F-31; the
  trajectory records step boundaries and committed messages.
- Requiring an external backend, cross-session analytics, or evaluation and
  dataset concepts.
- A product TUI surface for trajectory inspection (the web viewer is the
  inspection surface; trajectory is a settings-controlled web feature).
- Cross-process capture: hostd and the agent runtime run in one process today;
  revisit only if the processes split.
- Changes to compaction, usage accounting, approvals, or turn-runtime
  semantics.
- Runtime span instrumentation as a second capture path.

## Behavior and states

### Record shape (technology-neutral)

- Execution header: session, agent instance, run, execution, and turn
  identity; assembly version and digest.
- Assembly record: the assembled prompt (per content policy) and tool catalog
  identity.
- Model step records: request, response including thinking blocks, usage
  (input/output, cache-read/cache-write tokens, cost), started/finished
  timestamps, duration, and retries/fallbacks with per-attempt detail.
- Tool batch records: each call's arguments, status transitions (running,
  awaiting approval, completed, failed, cancelled), result content or error,
  and duration.
- Child run records: child agent identity, link to the child run's trajectory,
  and completion fragment reference.
- Terminal record: completed, failed, or cancelled with the reason.

### Two record categories, one durable journal

The journal records everything for a run as two sides of one record:

1. **Prompt assembly (input side)** — per-run construction of the model
   input: the assembled system prompt content, ordered resources, tool
   catalog identity and version. This is the new focus of the feature.
2. **Agent trajectory (interaction side)** — the run's ordered multi-turn
   interaction: system prompt reference, user messages, tool calls and
   results, assistant messages, and system notifications (lifecycle,
   approvals, errors, compaction). Model-step detail — request/response
   including thinking, timing, retries/fallbacks, and child-run links — rides
   along as per-step records in this side.

Both sides are keyed by the same run identity; a trajectory query returns them
joined as one run record — one run, two views. They are two sides of the same
coin: assembly describes the input constructed for the run, trajectory
describes what the run did with it.

Assembly and trajectory detail are recorded as observational event types in
the same journal stream as acknowledged facts. They are durable and replayable
like facts, but they never participate in the acknowledged session-state
projection, are best-effort (never block or alter a turn), and are ignorable
by readers that do not understand them.

### Capture semantics

- Records are produced at the production runtime step boundaries; there is no
  parallel debug capture path.
- Capture is best-effort: a record that cannot be durably written is dropped
  and reported as missing on the trajectory query for that run; the turn is
  never failed, delayed, or altered by capture.
- Concurrent agent runs produce isolated trajectories keyed by run identity.
- A model input/output that is never committed (failed, cancelled, or
  abandoned step) is recorded by the trajectory even though the journal never
  sees it.

### Restoration and lifecycle

- Trajectories survive hostd restart; restart is not a clearing event.
- An interrupted run keeps every record durably written before interruption;
  its trajectory queries as interrupted (no terminal record).
- There is no retention eviction: trajectory records persist like all journal
  records.

### Query behavior

- Listing runs: scoped by session and agent, newest first, bounded page with
  cursor.
- Fetching one run: full step graph, paged for large runs.
- A missing run returns an explicit error; a query never attaches a session,
  starts a run, or invokes the model gateway.

### Web viewer

- hostd serves read-only HTTP on loopback only; the static page and the
  trajectory endpoints come from the same process that owns the store.
- Endpoints: list runs (session/agent scope, paged), fetch one run (full step
  graph, paged), and the static viewer page. All are read-only.
- Live following uses SSE (`EventSource`, `text/event-stream`) from hostd to
  the browser: the page subscribes to a run's event stream and hostd pushes
  new trajectory records as they are durably written. The web viewer is a
  real-time viewer, not a polling client. SSE is the browser-native standard;
  a streamable-HTTP-style NDJSON endpoint for non-browser consumers is a
  later addition, not a viewer requirement.
- Large payloads are handled at render time (lazy load, collapse, scroll into
  view); stored records are never truncated.
- The page renders the step graph with foldable steps, content, and child-run
  links; plain static assets, no separate frontend toolchain.
- The per-run view renders the run's prompt assembly as a time-ordered card
  in the message stream (same card interaction and selection as messages) and
  as a matching timeline marker brick; there is no separate prompt tab
  (D-52).

### Observability integration

- OTel metrics (TTFT/TTFM, retry/fallback counters, token/cost) and unified
  logs remain; log records keep run/step correlation attributes because the
  trace context is no longer exported.
- The OTel span exporter is removed; internal tracing spans remain only as
  local console correlation and are not exported.
- D-46 GenAI content attributes are removed; the trajectory content policy is
  the only content capture.
- Escape hatch: if external trace UX is ever required, spans are derived from
  trajectory records in an exporter rather than re-added as runtime
  instrumentation.

## Acceptance criteria

- [ ] Every completed, failed, or cancelled execution yields a durable
      trajectory whose step graph contains assembly, model steps (request,
      response, thinking, usage, timing, retry/fallback), tool calls
      (arguments, status, result/error, duration), child-run links, and the
      terminal outcome.
- [ ] A trajectory survives hostd restart; an interrupted run preserves all
      records durably written before interruption and queries as interrupted.
- [ ] Capture is best-effort: a simulated capture failure does not fail,
      delay, or alter the turn, and the run's trajectory reports the dropped
      record.
- [ ] Query commands are read-only: listing/fetching does not mutate session
      state or invoke the model gateway; missing runs return explicit errors.
- [ ] Records are captured in full without truncation; oversized payloads are
      handled by the web viewer at render time, not by altering stored content.
- [x] Once the trajectory query path is verified, all D-30 prompt-debug code is
      deleted (capture, protocol command and result, TUI surface and tests);
      the web viewer shows the latest run's structured step graph instead
      (verified in V-49; the D-30 design and V-30 verification records are
      removed).
- [ ] The web viewer is served over loopback HTTP only, is read-only, and
      works without an external backend or frontend build toolchain.
- [ ] The web viewer follows a running turn in real time over SSE (or
      streamable HTTP) without polling.
- [ ] The OTel span exporter and D-46 GenAI content attributes are removed;
      metrics and unified logs remain with run/step correlation.
- [ ] Workspace fmt, clippy with warnings denied, and full tests pass.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where does trajectory content live? | Session journal, as observational event types alongside acknowledged facts | One durable record; assembly/trajectory detail is optional-event-class, replayable, and never part of session-state projection. |
| Who captures? | Agent runtime emits step events; hostd persists | hostd stays authoritative for durable state; capture points are the existing production step boundaries. |
| Content by default? | Always captured in full; no truncation, no disable switch | The journal already stores message bodies (including thinking) locally; sizes are naturally bounded by context/output/tool caps. |
| Retention? | None — trajectory records persist like all journal records | No eviction; the journal is already the unbounded durable record. |
| Replace prompt-debug? | Yes — delete all D-30 code after the trajectory query path lands | Trajectory subsumes D-30's data; a live latest-run view replaces the surface. |
| Viewer surface | Loopback web viewer (HTTP + SSE), enabled by `[trajectory] enabled` | Browser inspection; read-only, hostd-backed, no TUI surface. |
| Record categories | Prompt assembly (input side) and agent trajectory (interaction side), joined by run identity | One run record with two views; assembly is the new focus, trajectory extends existing journal events. |
| Fact vs observation | Acknowledged facts stay authoritative; assembly/trajectory detail is optional-event-class | Preserves F-31 replay and authority while letting the journal record everything. |
| OTel spans? | Removed as an export; trajectory is the causal graph | Avoids dual instrumentation and truncated span attributes; escape hatch derives spans from records later. |
| OTel metrics/logs? | Retained | Histograms/counters and unified logs are projections over the same runtime events; logs keep run/step correlation. |
| Live updates | SSE (`EventSource`) from hostd to the browser; no polling | Browser-native standard with auto-reconnect; hostd pushes records as they are durably written. |
| Streaming deltas? | Not recorded | Transient by F-31; trajectory fidelity is step boundaries plus committed messages. |
| Cross-process capture? | Deferred | In-process event flow today; revisit only if processes split. |

## Fusion decisions (codex-rs)

F-15 slices were derived from codex-rs (rollout, prompt debugging, turn
timing, OTel initialization). F-36 re-expresses them under one piko-native
record.

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| rollout (per-agent transcript paging) | kept (adapted) | Journal-backed rollout remains the committed-message projection; trajectory adds content-complete step records around it. |
| prompt-debug latest assembly capture | kept (adapted) | Becomes a query over the trajectory's latest run; the standalone in-memory D-30 capture is retired. |
| OTel span export / GenAI content attributes | rejected (as a capture path) | Trajectory is the content and causal truth; spans would be derived records, not runtime instrumentation. |
| turn timing / OTel metrics | kept | Metrics remain projections over runtime events. |

## Open questions

1. None. SSE is decided for the browser viewer; a streamable-HTTP-style NDJSON
   endpoint for non-browser consumers is deferred as a later addition.

## Reference evidence

- F-15 observability and its slices D-15 (tracing/metrics/logs), D-30 (prompt
  debugging), D-46 (OTel GenAI inspection), with verification V-30/V-46.
- F-31 durable session journal: host-owned append-only facts; the trajectory
  is deliberately separate.
- Protocol message model already carries thinking blocks and tool call/result
  content.
- Existing agent-runtime step boundaries (model step and tool batch consumers)
  as the natural capture points.
