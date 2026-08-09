# F-26: Protocol-neutral inference

> Status: implemented
> Priority: P0
> Source evidence: piko product direction; F-02 model gateway; F-25 native
> OpenAI-family model protocols

## Summary

piko exposes one protocol-neutral inference contract to every model consumer.
Callers provide a complete logical conversation and standard inference intent;
the model gateway decides whether a selected target requires full-history
replay, server-side continuation, or an opaque-state replay strategy. The
gateway returns only semantic output and an optional opaque checkpoint that
callers may persist and return without inspecting it. The contract remains
general enough to represent new model capabilities without adopting the wire
grammar of Responses, Chat Completions, or another provider API.

## Problem

Native protocol adapters solve wire correctness but do not by themselves
create a neutral public boundary. The current gateway still exposes artifacts
of particular protocols and duplicates some concepts:

1. Chat Completions reconstructs every request from history, while Responses
   may continue provider-side state. A caller must not branch on that
   difference or make semantic correctness depend on one protocol's state
   mechanism.
2. Continuation metadata exposes an adapter name and arbitrary adapter JSON.
   It is opaque by convention, not by contract.
3. Response IDs and output/content indexes appear in semantic output identity
   even though they are wire coordinates, not agent concepts.
4. Reasoning and tool options include loosely typed provider-shaped strings,
   making unsupported combinations easy to express and easy to drop silently.
5. The streaming `execute` and stateless `llm_call` entry points describe the
   same inference operation through different models.
6. Public model metadata mixes model identity, presentation, API location, and
   incomplete capability booleans.
7. Responses exposes upstream tools, durable background execution, conversation
   resources, resumable streams, and server-side compaction. Discarding these
   would lose useful capabilities, while exposing their resource DTOs would
   make the core OpenAI-specific.

Without a stronger boundary, every additional native protocol either leaks
new transport concepts upward or is forced through an OpenAI-shaped lowest
common denominator.

## User journeys

1. An agent executes the same logical conversation against a Chat Completions
   target and a Responses target. It submits the same inference shape and
   consumes the same semantic event categories; only llmd's internal wire plan
   differs.
2. A Responses target returns a reusable server-side checkpoint. The session
   persists it and supplies it with later history without learning its adapter
   or contents. llmd resumes at the correct logical boundary without
   duplicating input.
3. A checkpoint is stale, malformed, belongs to another target, or no longer
   matches the logical history. llmd ignores it and safely replays the complete
   logical conversation when the target supports replay.
4. A user changes provider, model, API surface, protocol profile, or relevant
   continuation configuration. The old checkpoint is never sent to the new
   target; the next inference starts from the logical conversation.
5. A caller requests reasoning, structured output, a modality, or a tool
   behavior that the resolved target cannot express. The call fails with a
   typed capability error before dispatch rather than silently weakening the
   request.
6. A host restart restores the transcript and opaque checkpoint. The next
   model step has the same semantics as a step performed before the restart.
7. An authorized target offers upstream search, retrieval, MCP, shell, computer,
   image, or deferred-tool capabilities. The caller observes typed activity,
   approvals, citations, and artifacts without constructing Responses tools.
8. A target supports durable background execution. A caller can detach,
   restore an opaque handle, resume after a cursor, and cancel without handling
   a provider response ID.

## In scope

- One public inference operation for streaming and assembled results.
- A protocol-neutral model reference, logical conversation, inference intent,
  event stream, terminal outcome, usage record, and error taxonomy.
- A complete logical conversation as the durable source of truth for every
  request.
- Optional opaque checkpoints as replay optimizations, never as public
  protocol structures.
- Target-bound checkpoint validation and deterministic selection of the most
  recent compatible checkpoint.
- Internal planning for full replay, server-side continuation, and
  opaque-state replay.
- Stable semantic identities for output items and tool calls without exposing
  provider response IDs or stream indexes.
- Structured model descriptors covering modalities, reasoning, tools,
  structured output, limits, and supported inference controls.
- Explicit capability validation with no silent loss of requested semantics.
- Streaming and non-streaming execution as delivery modes of the same request
  and result model.
- Safe persistence, restoration, redaction, and size bounds for opaque
  checkpoints.
- Typed capability semantics for caller-executed, upstream, and hybrid
  tools, deferred tool discovery, citations, and generated artifacts.
- A protocol-neutral optional durable-execution lifecycle: start, attach,
  resume after an opaque cursor, observe terminal state, and cancel.
- Internal mappings for provider conversation resources, response chaining,
  resumable streams, and provider compaction without making them durable
  conversation authority.

## Out of scope

- A public provider-plugin or protocol-adapter ABI.
- Exposing provider response, conversation, vector-store, file, or batch DTOs
  and identifiers directly to model consumers.
- Enabling every upstream tool in the first implementation slice; each
  externally acting capability still requires authorization, approval, and
  observability contracts.
- Making llmd the durable owner of sessions or transcripts.
- Treating provider-side state as the only copy of user-visible conversation
  history.
- Automatic fallback from one configured wire protocol to another.
- Arbitrary provider-specific request fields or untyped extension maps in the
  public inference contract.
- Guaranteeing that every model supports every neutral capability.
- Replacing hostd's user-visible model catalog and settings ownership.

## Behavior and states

### Logical conversation

The request contains an ordered logical conversation independent of transport
roles and resource IDs. It represents trusted instructions and context, user
content, assistant output, tool calls, and tool results. Content is composed
from typed parts such as text, image, audio, and file references when the
selected target supports them.

The logical conversation is sufficient to construct a correct replay request.
A provider-side checkpoint may improve continuity, caching, or reasoning
quality, but it does not replace the durable conversation. If a target exposes
state that cannot be safely reconstructed, inability to use that state is a
typed continuation failure rather than a silently incomplete request.

### Checkpoints

A completed assistant output may carry an opaque checkpoint. Consumers may
store, copy, and return the checkpoint but cannot observe or construct its
adapter, response IDs, reasoning state, or wire-policy fields.

llmd binds a checkpoint to the effective model target and the logical history
boundary it covers. It validates format version, target binding, boundary, and
size before use. Invalid or incompatible checkpoints never cross the network.
When several checkpoints exist, llmd chooses the newest compatible checkpoint
whose covered history is still present.

### Execution planning

For each inference, llmd resolves one target and produces one internal plan:

- **full replay** encodes the complete logical conversation;
- **server-side resume** sends a validated checkpoint plus only the uncovered
  conversation suffix;
- **opaque-state replay** reconstructs the request with protocol-private state
  retained in the checkpoint.

The plan is not exposed to callers. All plans must preserve the same logical
ordering, tool-call relationships, requested controls, and observable output
semantics. Authentication material cannot change the plan after target
resolution.

### Model and capability semantics

A model is identified by a provider-scoped reference. Provider qualification
prevents ambiguity but does not alter the inference request shape. Public
model descriptors contain presentation metadata, supported input/output
modalities, tool behavior, reasoning controls, structured-output support, and
limits. API surfaces, credentials, endpoints, and wire protocols are target
configuration, not model presentation fields.

Inference controls use closed semantic types where piko relies on their
meaning. An adapter maps them to a wire representation or reports an
unsupported capability. Provider-specific strings do not flow through the
agent runtime as substitutes for modeled controls.

### Advanced capabilities

The neutral model distinguishes capability semantics from execution locus. A
tool or operation may be caller-executed, upstream-executed, or hybrid.
Upstream search, retrieval, MCP, shell, computer use, image generation,
deferred tool loading, and programmatic tool execution use typed requests and
emit typed activity, approval, source, citation, and artifact events where
applicable.

Upstream execution is opt-in and policy-gated because it does not pass through
orchd's local tool executor. Catalog support alone never authorizes network,
code, computer, file, or remote MCP activity.

Upstream calls, results, sources, citations, and artifacts that affect later
turns remain part of the logical conversation in semantic form. Wire-only
execution records needed for replay remain inside the checkpoint.

Provider conversation objects, response chaining, and server-side or
standalone compaction are internal state backends. llmd may encode their state
inside checkpoints, but hostd's logical transcript remains authoritative.
Provider compaction may optimize a later wire context; it cannot silently
rewrite the user-visible transcript.

Durable background execution uses an opaque execution handle and event cursor.
Callers attach, resume, observe terminal status, and cancel through neutral
operations. Polling, stream reconnection, and provider resource IDs stay
private to llmd.

### Output and identity

The gateway emits semantic text, reasoning, refusal, tool-call, usage, and
terminal events. Item and tool-call identities are piko semantic identities.
Provider response IDs, item IDs used only for continuation, choice indexes,
output indexes, and content indexes stay inside llmd.

Streaming and assembled execution produce equivalent semantic results for the
same successful upstream response. Delivery mode does not change checkpoint,
usage, terminal-status, or capability semantics.

### Failure, cancellation, and restoration

Target resolution, unsupported capability, authentication, continuation,
transport, timeout, rate limit, upstream, protocol, and cancellation failures
remain distinguishable. Checkpoint rejection occurs before dispatch. A safe
replay fallback is observable through diagnostics but does not require caller
control flow.

Cancellation aborts planning, retry waits, dispatch, and stream consumption.
No checkpoint from an incomplete or cancelled response becomes durable.
Restored checkpoints pass the same validation as newly produced checkpoints.

## Acceptance criteria

- [x] `orchd` and hostd model-call consumers use one inference request shape
      for Chat Completions and Responses targets and contain no protocol-kind
      branch.
- [x] The public request always carries the complete logical conversation;
      llmd alone selects full replay, server-side resume, or opaque-state
      replay.
- [x] Checkpoint payloads expose no adapter name, provider response ID, output
      item ID, encrypted reasoning field, continuation policy, or arbitrary
      JSON to consumers.
- [x] A valid Responses checkpoint sends only the uncovered logical suffix and
      never duplicates transcript input.
- [x] Chat Completions executes the same request by full replay and requires no
      synthetic checkpoint.
- [x] A malformed, stale, wrong-target, wrong-version, or history-mismatched
      checkpoint is never dispatched and safely falls back to full replay when
      replay is supported.
- [x] A target whose required state cannot be safely replayed returns a typed
      continuation error instead of sending incomplete context.
- [x] Changing provider, model, API surface, protocol profile, or continuation
      configuration invalidates the previous checkpoint.
- [x] Session persistence round-trips an opaque checkpoint without inspecting
      or rewriting its payload.
- [x] Public semantic events and identities contain no Responses resource IDs,
      Chat choice indexes, output indexes, content indexes, or adapter names.
- [x] Requested reasoning, modality, tool, structured-output, and delivery
      semantics are either represented by the selected target or rejected
      before dispatch; no requested feature is silently dropped.
- [x] Streaming and assembled delivery produce equivalent semantic items,
      usage, terminal outcome, and checkpoint for equivalent fixtures.
- [x] The legacy `llm_call` abstraction is removed after all callers use the
      unified inference operation.
- [x] Public model presentation metadata contains no base URL, credentials,
      endpoint, protocol, or continuation policy.
- [x] OpenAI API-key/OAuth and DeepSeek Chat/Responses targets pass the same
      protocol-neutral contract tests.
- [x] Opaque checkpoint parsing is bounded, rejects malformed data, and never
      logs checkpoint payloads or credential material.
- [x] Capability discovery distinguishes caller-executed, upstream, and
      hybrid tools without exposing provider tool JSON.
- [x] Upstream tool activity, approvals, citations, sources, and artifacts have
      semantic event forms; hostd policy is required in addition to model
      capability support.
- [x] Conversation IDs, response IDs, compaction items, background IDs, and
      stream sequence numbers remain inside opaque llmd handles or checkpoints.
- [x] A durable execution can be detached, restored, resumed without duplicate
      events, and cancelled when the target advertises that capability.
- [x] Provider-side compaction may change a later wire plan but never mutates
      or replaces hostd's durable logical transcript.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Does protocol-neutral mean provider-less? | No. Model references remain provider-scoped; request and result semantics do not vary by provider or protocol. | Provider qualification is required for unambiguous target and auth resolution. |
| Who owns durable conversation state? | hostd owns persistence; llmd owns checkpoint meaning. | Preserves the host/orchestrator split while hiding wire state. |
| Is a checkpoint authoritative? | No. The logical conversation is authoritative; a checkpoint is an opaque execution optimization. | Prevents provider retention and protocol changes from becoming data-loss risks. |
| May callers inspect checkpoint contents? | No. They may only persist and return the token. | Prevents adapter coupling from crossing the llmd boundary. |
| What happens when safe replay is impossible? | Fail with a typed continuation error. | Silent partial replay would violate conversation correctness. |
| Are provider-specific option maps allowed? | No public untyped passthrough. New required semantics are modeled explicitly. | Avoids protocol leakage and unverifiable capability behavior. |
| Are streaming and non-streaming separate operations? | No. They are delivery modes of one inference operation. | Keeps request and result semantics identical. |
| Are Responses advanced features discarded for neutrality? | No. General semantics enter typed capability and lifecycle models; provider resources remain private. | Neutrality must preserve capability, not impose a lowest common denominator. |
| May upstream tools bypass piko policy? | No. Capability support and execution authorization are independent gates. | Upstream execution can perform externally acting operations outside orchd's local executor. |
| Can provider compaction replace piko history? | No. It is an opaque wire-context optimization. | hostd remains authoritative and provider retention cannot be the only durable state. |

## Resolved implementation choices

1. The first implementation slice retains text and the existing image-input
   support. Other input and output modalities remain capability-model entries
   until a concrete runtime consumer is specified.
2. Structured-output intent lands as a typed, capability-validated inference
   option and is encoded by both current adapters. Callers cannot supply raw
   provider response-format JSON.

Acceptance evidence is recorded in
[V-38](../verification/V-38-protocol-neutral-inference.md).

## Reference evidence

- [F-02: Model gateway](F-02-model-gateway.md)
- [F-25: Native OpenAI-family model protocols](F-25-native-openai-model-protocols.md)
- [D-37: Native OpenAI-family model protocols](../design/D-37-native-openai-model-protocols.md)
- [ADR-009: First-class model targets](../decisions/ADR-009-first-class-model-targets.md)
- [OpenAI Responses conversation state](https://developers.openai.com/api/docs/guides/conversation-state)
- [OpenAI Responses background mode](https://developers.openai.com/api/docs/guides/background)
- [OpenAI Responses compaction](https://developers.openai.com/api/docs/guides/compaction)
- [OpenAI Responses tools](https://developers.openai.com/api/docs/guides/tools)
