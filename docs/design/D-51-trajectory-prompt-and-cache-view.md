# D-51: Trajectory prompt assembly and cache debugging view

> Status: superseded by [ADR-029](../decisions/ADR-029-retire-trajectory-web-viewer.md)
> Implements: [F-36](../features/F-36-agent-run-trajectory.md) (viewer surface; retired)
> Decisions: none new

## Goal

Give the trajectory web viewer a first-class Prompt view that renders the
frozen run assembly — semantic prompt blocks, tool catalog, and cache plan —
plus per-model-step provider cache effectiveness (cached / written tokens).
The recorded cache plan becomes an actionable debugging surface instead of
metadata that is stored and never displayed.

## Context: what exists today and what is missing

### The assembly record is content-complete; model-step requests are redacted

`trajectory.assembly` is the only durable record containing the full production
prompt: `SemanticRunPrompt` (blocks with kind / authority / trust / source /
cache scope / content / content digest), the resolved tool catalog, and the
cache plan (`prefix_segments`, `semantic_prefix_digest`). The
`trajectory.model_step` request is redacted to message ids and kinds
(`llmd/src/redaction.rs`), so the viewer cannot reconstruct the prompt from
model steps — it must render the assembly record.

### The cache plan is computed but never consumed

The assembler computes prefix segments by `CacheScope`
(`GlobalStable`, `OperatorStable`, `AgentStable`, `CatalogStable`,
`ResourceSnapshot`; `RunDynamic` blocks are excluded from the segment plan) and
records `semantic_prefix_digest`. Today these values are stored in the
trajectory and logged in spans; **no llmd code reads them when encoding
requests**. The two protocol adapters (Responses, Chat Completions) flatten
all blocks into one instructions string and emit no provider cache annotations
— which is correct for OpenAI-family providers (caching is automatic and
cannot be annotated), but it means the plan is informational only.

### Cache tokens are decoded and priced, but invisible in the trajectory

Both adapters decode provider-reported cache tokens into
`Usage.cache_read` / `cache_write` (including DeepSeek's
`prompt_cache_hit_tokens`), and billing prices them at cache-specific rates.
However, `TrajectoryModelStepRecord` carries no usage: the finish record only
adds `finished_at` / `duration_ms`. A user therefore cannot see, per step, how
many input tokens were served from cache, how many were written, or what the
provider-side hit ratio was.

## Proposed design

### Data model delta: per-step usage on the model-step record

`piko-protocol`: add an optional `usage` field to
`TrajectoryModelStepRecord`:

```text
usage: Option<Usage>   // input, output, cache_read, cache_write, cost
```

`serde(default, skip_serializing_if = "Option::is_none")` keeps old journal
events readable and new records backward-compatible.

`piko-llmd`: the executor's `ModelStepCapture` observes the step stream and
retains the final `InferenceEvent::Usage` (the existing middleware already
consumes the same event for accounting). `wrap_model_step_finish` writes the
finish record with that usage. If the consumer abandons the stream, no finish
record is written and the step stays `started` — unchanged behavior.

`TrajectoryModelStepRecord.message_id` links each step to the committed
assistant message. The orchestrator pre-assigns that id
(`runtime_assistant_message_id(execution_id, step_id)`) before dispatch, so it
is threaded through `InvocationContext.step_message_id` into the start and
finish records — a deterministic link, never a positional or temporal
heuristic. This is the join the Conversation view uses when it attaches
per-call usage to message cards.

`TrajectoryRun.messages` becomes `Vec<TrajectoryMessage>`: a flattened wrapper
that adds `messageId` to each committed message without changing the `Message`
wire shape. The id is read from the durable `message_committed` envelope (which
always carried it); only the trajectory projection was dropping it.

`piko-hostd`: no query change; model-step records already round-trip through
`TrajectoryRecord::ModelStep` into the run detail.

### Viewer: run detail tabs

The run detail area (currently runs-strip → timeline → messages) gains two
tabs:

```text
Conversation | Prompt
```

- **Conversation** keeps the existing timeline + message list and gains the
  per-call usage strips, the run summary bar, and the step bricks described
  below.
- **Prompt** is the assembly view below.
- **Per-call usage lives on Conversation messages**, not in the Prompt tab:
  each assistant card carries a collapsed "model call" strip joined by
  `messageId`, the run-level usage summary sits in a bar above the timeline,
  and model-step records appear as timeline bricks.

The Prompt view follows the D-50 invariants: DOM + native scroll with zero JS
on scroll, store-slice re-renders only, all colors/dimensions from CSS custom
properties, and no full-tree rebuilds.

### Prompt tab

#### Assembly header (always visible)

- Assembly version, `source_digest` (short display, full copy button),
  `semantic_prefix_digest` (short display, full copy button).
- Cache policy (`ProviderDefault` / `Ephemeral` / `Extended` / `Disabled`).
- Scale line: N blocks, N tools, total character count.
- One-line clarification addressing the recurring confusion: blocks are frozen
  at run start; transcript context/user messages are injected separately and
  are not part of the frozen prompt.

#### Cache plan bar

A horizontal stacked bar, one segment per `prefix_segments` entry:

- Segment color by `CacheScope` (new CSS tokens, e.g.
  `--cache-global-stable`, …, `--cache-run-dynamic`, `--cache-no-cache`).
- Segment width proportional to the summed character count of its blocks.
- Hover: tooltip with scope, segment digest, block count, character share.
- Click: highlights the corresponding blocks in the list below (scrolls the
  first block into view).
- Legend row plus a "copy segment digests" button.

This directly answers "which parts of the prompt are stable across runs and
where do breakpoints fall" without needing provider internals.

#### Block list (main content)

Each block is a collapsed card:

```text
[Instruction] [Platform] [Trusted] [GlobalStable]
platform.policy · compiled:piko/platform-policy@v0.x · 2.1k chars
[content digest short]                          ▸ expand
```

- Badges: kind, authority, trust, cache scope.
- Source line: `kind:locator@version`, character count, content digest short.
- Expand reveals full content (`white-space: pre-wrap`, monospace) plus copy
  content and copy digest actions.
- Filter chips with counts by kind / authority / trust; group toggle
  **flat** (rendered order) / **grouped** (by authority).
- Content stays collapsed by default: metadata-first, no first-paint cost
  proportional to prompt size.

#### Tool catalog

Collapsible section: summary line (N tools, catalog digest, contributing
`PromptSource`s), then per-tool entries with name, version, provenance,
description, and input schema JSON (collapsed by default).

#### Per-call usage on conversation messages

- **Message cards**: an assistant message produced by a model step gets a
  collapsed `details` strip — "model call · provider/model · duration" —
  expanding to `input / cache_read (hit ratio) / cache_write / output / cost`,
  plus retry/fallback markers. Steps without a committed message stay visible
  on the timeline but have no strip.
- **Run summary bar**: one line above the timeline showing step count, total
  input, total cached with hit ratio, written, output, and cost. Hostd
  computes the rollup (`TrajectoryRunSummary.usage`) at query time; the viewer
  only formats it — bookkeeping authority stays in hostd (F-32), the viewer
  never aggregates records.
- **Timeline bricks**: each model step renders as a `step` brick at
  `started_at`; clicking one selects the assistant message it produced (via
  `messageId`), and steps without a message are still inspectable through the
  brick tooltip.

#### Raw JSON

Toggle showing the full `TrajectoryAssemblyRecord` JSON with a copy button
(power-user fallback, matching D-50's "export run JSON" future extension).

### Timeline bricks

The timeline gains two additions:

- A single `prompt` marker at the assembly `recorded_at`; clicking it switches
  to the Prompt tab. It is a marker, not a track of per-block bricks: block
  content belongs in the detail view, not in the canvas.
- A `step` brick per model step at `started_at`, colored by a dedicated
  `--role-step` token, clickable through to the produced assistant message.

## Cache debugging semantics

The view is descriptive, not prescriptive:

- Provider-reported cache tokens (`cache_read` / `cache_write`) are ground
  truth for what actually hit.
- The cache plan bar is a reference for what *should* be stable: blocks with
  stable scopes (`GlobalStable` … `CatalogStable`). Character share is an
  approximation, not a token count; the view labels it as such.
- `semantic_prefix_digest` lets users compare runs manually and see whether
  the semantic prefix stayed byte-stable.

No code asserts "plan mismatch" from these numbers; that would require a
tokenizer and provider cache semantics we do not own.

## Constraints and non-goals

- No provider cache-control emission (Anthropic-style breakpoints). The
  current providers (OpenAI-family: Responses, Chat Completions, DeepSeek)
  cache automatically and accept no annotation. Consuming `prefix_segments`
  to emit `cache_control` is future work that must land with Anthropic
  protocol support, not before.
- No llmd change to reshape the wire encoding from the cache plan.
- No cross-run prompt diffing or cache-hit history across sessions (future
  extension).
- Viewer stays read-only loopback, no build toolchain, no new API endpoints:
  `TrajectoryRun` already carries `assembly`, `records`, and `messages`.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | `TrajectoryModelStepRecord.usage: Option<Usage>` (optional, backward-compatible); `TrajectoryMessage` adds `messageId` to `TrajectoryRun.messages` via flatten; `TrajectoryRunSummary.usage: Option<TrajectoryRunUsage>` carries the host-computed rollup |
| `piko-llmd` | `ModelStepCapture` retains final step `Usage`; finish record includes it |
| `piko-hostd` | Query computes the run-level usage rollup; viewer assets: tabs, prompt panel, cache bar, per-call message strips, run summary bar, step bricks, raw JSON |
| `piko-orchd` | none |

## Reusable infrastructure

- No `island-rs` change required.
- D-50 module layout is extended with `js/prompt.js` (pure derivation +
  renderers for the Prompt tab); `store.js` gains a `prompt` slice and tab
  state; `timeline.js` gains the prompt marker.

## Failure and cancellation

- Abandoned step stream: no finish record, no usage — same as today; the
  viewer shows the step as `started` with `—` usage.
- Provider without cache usage fields: `cache_read`/`cache_write` decode to 0;
  hit ratio renders `—` when input is 0.
- Old journal events without `usage`: field is absent; viewer renders `—`.
- Oversized block content is handled at render time (collapse + expand),
  never by altering stored records (F-36 invariant).

## Verification

- Protocol: serde round-trip for `TrajectoryModelStepRecord` with and without
  `usage`.
- llmd: a fake step stream emitting a final `Usage` event yields a finish
  record carrying that usage; an abandoned stream yields no finish record.
- Hostd: a journal with a model-step event including usage decodes through
  `TrajectoryQuery::fetch_run` unchanged.
- Viewer: manual verification against a fixture run — cache bar widths match
  block character shares, filter/group behave, step stats render, no
  full-tree rebuild on tab switch (D-50 invariants).
- Workspace gates: `cargo fmt --all`, clippy with `-D warnings`, full tests.
- F-36 PRD is updated to list usage on the model-step record when this lands.

## Alternatives considered

- **Right-side drawer for the prompt** — rejected: too narrow for long block
  content; tabs give full width and a natural home for future views (steps,
  records).
- **Collapsible prompt section above the timeline** — rejected: long prompts
  push the conversation off the page; tab separation keeps both views intact.
- **Compute expected cache-hit tokens from the plan** — rejected for this
  slice: character-to-token conversion and provider cache semantics are not
  owned by piko; the view stays descriptive (provider usage is ground truth).
- **Add a prompt track of bricks to the timeline canvas** — rejected: brick
  labels cannot carry block content; a single marker + detail tab is clearer.

## Rollout

1. Protocol field + llmd usage capture with tests; update F-36 PRD model-step
   record list.
2. Viewer tabs + assembly header + block list + tool catalog + raw JSON
   (pure presentation; fixture-driven manual verification).
3. Cache plan bar + per-call message strips + run summary bar + timeline
   prompt marker and step bricks.
4. Polish pass against D-50 invariants; workspace gates.
