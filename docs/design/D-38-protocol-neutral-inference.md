# D-38: Protocol-neutral inference boundary

> Status: implemented
> Implements: [F-26](../features/F-26-protocol-neutral-inference.md)
> Decisions: [ADR-009](../decisions/ADR-009-first-class-model-targets.md)

## Goal

Replace the remaining protocol-shaped llmd public surface with one semantic
inference boundary. `orchd` submits a complete logical conversation and typed
intent. llmd resolves the target, validates capabilities, chooses a replay or
resume plan, invokes the private protocol adapter, and returns semantic events
plus an opaque checkpoint. `hostd` remains authoritative for persistence;
neither hostd nor orchd interprets checkpoint contents.

This design refines the public-boundary and continuation portions of D-37. It
does not change ADR-009's provider, model, API-surface, authentication, or
target-resolution relationships.

## Constraints and non-goals

- `hostd` remains authoritative for durable user-visible state, settings,
  model selection, and authentication configuration.
- `orchd` owns the agent loop, logical transcript, tool scheduling, and
  semantic commit boundaries.
- `piko-llmd` owns inference semantics at its public boundary, target planning,
  checkpoint encoding, capability validation, adapters, and transport.
- `piko-protocol` remains a DTO-only leaf. It may carry an opaque checkpoint
  token needed for persistence but cannot define adapter state.
- The complete logical transcript remains available on every request. llmd is
  not a session database and does not retain mutable conversation state
  between calls.
- No compatibility wrapper remains for `llm_call`, adapter-tagged
  continuation JSON, or response-coordinate identities.
- No cross-protocol runtime fallback is introduced.
- The protocol-adapter trait remains private and is not a plugin ABI.
- Adding a neutral type is not permission to expose arbitrary provider
  extensions through JSON.

## Design

### Boundary overview

```text
hostd persistence                  orchd runtime
  transcript + opaque checkpoint     logical conversation + intent
             │                                  │
             └──────────────┬───────────────────┘
                            ▼
                    llmd::InferenceGateway
                            │
                 resolve ModelRef + auth route
                            │
                 validate target capabilities
                            │
                  ConversationPlanner
                 ┌──────────┼───────────┐
                 ▼          ▼           ▼
             FullReplay  ServerResume  OpaqueReplay
                 └──────────┼───────────┘
                            ▼
                    private adapter DTOs
                            │
                            ▼
             semantic events + opaque checkpoint
```

The public API contains no protocol profile, operation path, endpoint,
adapter identifier, provider response resource, or transport DTO.

### Public inference types

The llmd-owned public model is organized by semantic responsibility:

```rust
pub struct ModelRef {
    pub provider: ProviderId,
    pub model: ModelId,
}

pub struct InferenceRequest {
    pub model: ModelRef,
    pub conversation: Conversation,
    pub tools: Vec<ToolDefinition>,
    pub options: InferenceOptions,
    pub context: InvocationContext,
}

pub struct InvocationContext {
    pub session_id: String,
    pub agent_instance_id: String,
    pub run_id: String,
    pub step_id: String,
}
```

`ModelRef` is provider-qualified but protocol-neutral. It replaces parallel
`provider: String` and `model: String` fields at the gateway boundary and
converts directly to ADR-009's internal `ModelKey`.

Correlation fields move into `InvocationContext`; middleware and telemetry
consume that context without treating it as model input. `InferenceOptions`
contains typed semantic controls, initially reasoning effort, delivery mode,
tool choice, and maximum output tokens. Each field has explicit unspecified,
supported, and unsupported behavior.

Foreground and durable execution share one start operation. Durable targets
may additionally return an opaque handle that supports restoration:

```rust
#[async_trait]
pub trait InferenceGateway: Send + Sync {
    async fn start(
        &self,
        request: InferenceRequest,
        cancel: CancellationToken,
    ) -> Result<InferenceExecution, InferenceError>;

    async fn attach(
        &self,
        handle: OpaqueExecutionHandle,
        after: Option<OpaqueEventCursor>,
        cancel: CancellationToken,
    ) -> Result<InferenceEventStream, InferenceError>;

    async fn cancel(&self, handle: OpaqueExecutionHandle)
        -> Result<InferenceStatus, InferenceError>;
}
```

Assembled results are produced by a library collector over the same event
stream. Foreground adapters return no durable handle; handle operations fail
capability validation for them. There is no second virtual `llm_call` method.
Callers such as
compaction and guardian construct a smaller `InferenceRequest` and use the
collector.

### General conversation model

The conversation model represents agent semantics instead of provider roles:

```rust
pub struct Conversation {
    pub instructions: SemanticRunPrompt,
    pub items: Vec<ConversationItem>,
}

pub struct ConversationItem {
    pub id: ConversationItemId,
    pub kind: ConversationItemKind,
    pub checkpoint: Option<OpaqueModelCheckpoint>,
}

pub enum ConversationItemKind {
    Context { content: Content, trust: ContentTrust, source: PromptSource },
    User { content: Content },
    Assistant { content: Vec<AssistantPart> },
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}
```

`ConversationItemId` is piko-authored and stable across persistence and
restoration. It is not copied from a provider response. Existing runtime
message IDs are reused where they identify a committed transcript item;
projection code assigns deterministic child IDs for separately addressable
assistant parts and tool calls.

`Content`, `AssistantPart`, `ToolCall`, and `ToolResult` use typed semantic
parts. The initial migration preserves existing text, image, refusal,
reasoning, function call, and function result behavior. Audio, file, and
structured output variants are added only with a consumer and capability
contract; the type layout reserves those semantic categories without a raw
provider JSON escape hatch.

Tool calls and results include their execution locus. Hosted activity that
affects future turns is retained as semantic conversation items; provider-only
replay records are retained solely in the opaque checkpoint.

A checkpoint is attached only to the completed assistant item whose prefix it
covers. This makes the logical boundary durable without adding an independent
mutable session cursor. Compaction or transcript projection that removes the
anchored prefix also removes eligibility for that checkpoint.

### Opaque checkpoint carrier

The shared DTO is deliberately smaller than the current continuation
envelope:

```rust
pub struct OpaqueModelCheckpoint {
    token: String,
}
```

The field is serializable for session storage but has no public adapter or
JSON-state accessors. Outside llmd it supports clone, equality, redacted debug,
and serialization only. Construction and inspection use llmd APIs.

The token is a bounded, base64url-encoded, versioned envelope containing
llmd-private data:

```text
format version
target fingerprint
covered conversation-item ID
covered-prefix digest
adapter checkpoint payload
```

The target fingerprint covers provider, model, API surface, protocol profile,
and continuation configuration. It excludes expiring credential material.
The prefix digest is computed from the canonical semantic conversation, not
wire JSON. Decoding rejects unknown versions, excessive decoded size,
malformed fields, target mismatch, missing anchor items, and prefix mismatch.
Checkpoint payloads are never logged or included in telemetry snapshots.

The envelope is opaque, not a security credential. Local tampering causes
validation failure and replay; adapters must still treat every decoded field
as untrusted input. If a future provider checkpoint contains a bearer secret,
that provider requires an encrypted-at-rest credential design before support.

### Conversation planning

After resolving `ResolvedModelTarget`, `ConversationPlanner` scans assistant
items newest-first and selects the first checkpoint that:

1. decodes within the size and version limits;
2. matches the resolved target fingerprint;
3. names an item still present in the conversation;
4. matches the canonical digest through that item; and
5. is usable by the selected adapter's continuation policy.

It then produces one llmd-private plan:

```rust
enum ConversationPlan {
    FullReplay { items: Range<usize> },
    Resume { checkpoint: AdapterCheckpoint, suffix: Range<usize> },
    OpaqueReplay { checkpoint: AdapterCheckpoint, items: Range<usize> },
}
```

The exact Rust representation may use slices instead of ranges. The important
invariant is that adapters receive a validated plan and cannot independently
scan public transcript metadata.

Chat Completions always receives `FullReplay`. Responses with
`previous_response_id` receives `Resume`, and the suffix begins strictly after
the checkpoint anchor. Stateless Responses receives either `FullReplay` or
`OpaqueReplay` according to the frozen target profile. DeepSeek uses the same
planner; it has no provider-specific branch above target resolution.

If no compatible checkpoint exists, the planner selects `FullReplay` when the
target can safely reconstruct the request. A target policy may declare
replay-required state; in that case the planner returns
`ContinuationUnavailable` instead of dispatching partial context. Invalid
checkpoint data never becomes an authentication, endpoint, or protocol input.

### Output model and semantic identity

Replace `ModelEvent`, `ModelResult`, and `ItemIdentity` with inference names
and piko-authored identities:

```rust
pub enum InferenceEvent {
    TextDelta { item_id: OutputItemId, delta: String },
    ReasoningDelta { item_id: OutputItemId, delta: String },
    RefusalDelta { item_id: OutputItemId, delta: String },
    ToolCallDelta { call_id: ToolCallId, delta: ToolCallDelta },
    Usage(InferenceUsage),
    Checkpoint(OpaqueModelCheckpoint),
    Completed(FinishReason),
    Error(InferenceError),
}
```

The adapter maps response resources, choice indexes, output indexes, and
content indexes into its private stream state. llmd assigns stable semantic
IDs before emitting the first delta for an item. `ToolCallId` represents the
logical call relationship and remains stable when the provider supplies a
usable call ID; provider-only continuation IDs remain private.

Exactly one checkpoint may be emitted for a successfully completed assistant
item. It is emitted only after the adapter has validated its terminal state
and before `Completed`. A cancelled, incomplete, or failed response emits no
durable checkpoint. The assembled collector enforces the same ordering.

### Advanced capability families

The public tool algebra includes an execution locus: caller, hosted, or
hybrid. Typed variants cover search/retrieval, remote MCP, shell, computer use,
image generation, deferred tool discovery, and programmatic tool execution.
Their configuration uses semantic resource references and approval policy;
raw Responses tool objects remain private.

Hosted and hybrid activity uses neutral tool progress/result, approval,
citation/source, and artifact events. Catalog support, hostd authorization,
and adapter support are separate gates. Remote MCP or hosted shell therefore
cannot bypass piko policy merely because a model advertises the capability.

Responses Conversations, response chaining, and encrypted replay are private
`ConversationPlan` backends. Server-side and standalone Responses compaction
produce adapter checkpoint payloads; they do not mutate the semantic
transcript or replace hostd compaction.

Background Responses maps to `OpaqueExecutionHandle`; stream sequence numbers
map to `OpaqueEventCursor`. llmd privately performs create, retrieve/poll,
stream reattachment, and cancel. Handles bind to target and auth route, are
redacted and size-bounded like checkpoints, and are persisted only when a
runtime feature elects durable execution.

### General model descriptors

Separate public presentation/capability metadata from executable targets:

```rust
pub struct ModelDescriptor {
    pub model: ModelRef,
    pub display_name: String,
    pub capabilities: ModelCapabilities,
    pub limits: ModelLimits,
}

pub struct ModelCapabilities {
    pub input_modalities: BTreeSet<InputModality>,
    pub output_modalities: BTreeSet<OutputModality>,
    pub tools: ToolCapabilities,
    pub reasoning: ReasoningCapabilities,
    pub structured_output: StructuredOutputCapabilities,
    pub delivery: DeliveryCapabilities,
    pub hosted_tools: HostedToolCapabilities,
    pub state: StateCapabilities,
    pub execution: ExecutionCapabilities,
}
```

Capabilities describe semantic support and valid control values, not endpoint
features. For example, reasoning declares supported effort levels and summary
behavior rather than one `reasoning: bool`; tool capabilities declare tool
choice and parallel-call behavior. State capabilities declare replay,
provider-state, and compaction support; execution capabilities declare
foreground, durable, resumable-stream, polling, and cancellation behavior.
`ModelLimits` holds context, output, media, checkpoint, and tool-definition
limits with explicit unknown values.

`ApiSurface`, `ProtocolProfile`, auth methods, base URLs, headers, and
continuation policy remain in target/catalog types. The current public
`Model.base_url` is removed. Catalog loading constructs both
`ModelDescriptor` and target profiles from one validated manifest so their
model identities cannot diverge.

### Capability validation

Validation runs after target resolution and before checkpoint planning or
transport dispatch. It compares `InferenceRequest` intent with the effective
descriptor and target capabilities. Unsupported requested behavior returns a
typed `UnsupportedCapability` containing the semantic capability name; it
does not expose an adapter field name.

Adapters may perform additional wire invariants, but cannot silently remove a
validated instruction, modality, tool definition, reasoning request,
structured-output constraint, or output limit. A catalog claiming support
that the adapter cannot encode is a configuration error covered by adapter
contract tests.

### Error model

`InferenceError` retains target, authentication, transport, timeout, rate
limit, upstream, protocol, and cancellation classes and adds explicit
checkpoint classes:

- `CheckpointRejected` is recoverable internally when full replay is safe;
- `ContinuationUnavailable` is terminal when required state cannot be safely
  reconstructed.

Recoverable checkpoint rejection is recorded as redacted diagnostic metadata,
not emitted as an error followed by success. Provider resource IDs and token
payloads are excluded from error display and telemetry.

### Ownership and persistence

`orchd` creates semantic item IDs, submits `InferenceRequest`, assembles
assistant output, and attaches the emitted opaque checkpoint without
inspection. It never selects a continuation policy or adjusts transcript
replay based on checkpoint presence.

`hostd` persists the assistant item and token through its existing append-only
session channel. It does not decode, migrate, or refresh token contents.
Schema changes, if required by the DTO replacement, follow the project's
no-legacy-migration policy rather than accepting both old and new checkpoint
shapes.

llmd owns token versioning and decoding. Because llmd is request-stateless, a
restart needs no in-memory reconstruction beyond the supplied conversation.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Replace adapter-tagged continuation DTO with an opaque checkpoint carrier; remove API location from public model presentation DTO; carry stable semantic item IDs where persistence requires them. |
| `piko-hostd` | Persist opaque checkpoints unchanged; expose generalized model descriptors; migrate compaction and guardian calls to the unified inference operation. |
| `piko-orchd` | Build `InferenceRequest`, preserve semantic IDs, consume only neutral events, attach checkpoints without inspection, and remove response-coordinate handling; hosted execution remains policy-gated. |
| `piko-llmd` | Own neutral types, descriptors, checkpoint/handle codecs, conversation planning, capability validation, execution lifecycle, and private adapter state. |

## Reusable infrastructure

- No `island-rs` change required.

## Failure and cancellation

- Invalid checkpoint: reject before dispatch; use full replay only when the
  target declares it safe.
- Required state unavailable: return `ContinuationUnavailable`; do not send a
  shortened or partially reconstructed conversation.
- Unsupported intent: return `UnsupportedCapability` before transport.
- Target/auth mismatch: preserve ADR-009's fail-closed behavior.
- Cancellation during planning, retry, open, or consume: terminate once and
  emit no checkpoint from partial output.
- Stream failure after observable output: preserve F-02's no-restart rule.
- Persistence failure: hostd reports its existing storage failure; llmd does
  not retain a hidden replacement checkpoint.
- Restoration: validate the restored token exactly like a newly supplied
  token; do not trust storage origin.
- Durable attach: deduplicate events through the opaque cursor and reject a
  handle whose target or auth route no longer matches.
- Hosted tools: require advertised support and host authorization; denial
  cannot be reinterpreted as permission to execute a local tool.

## Verification

The landed verification record is
[V-38](../verification/V-38-protocol-neutral-inference.md).

- Unit tests for canonical conversation hashing, target fingerprints, token
  size/version validation, newest-compatible selection, suffix boundaries,
  safe replay decisions, and semantic capability validation.
- Property tests that arbitrary checkpoint strings never panic, allocate past
  the configured bound, or appear in error/debug output.
- Adapter contract fixtures that run one neutral request through Chat
  Completions full replay, OpenAI Responses resume, OpenAI Responses opaque
  replay, and DeepSeek Responses replay.
- Equivalence tests comparing streaming collection with assembled execution
  for semantic items, usage, finish reason, and checkpoint.
- Integration tests for target/model/protocol switches, compaction removing a
  checkpoint boundary, host restart, malformed persisted tokens, and
  non-replayable continuation failure.
- Lifecycle tests for background detach/attach, polling, cursor deduplication,
  terminal restoration, cancellation, hosted-tool policy denial, and typed
  citation/artifact projection.
- Architecture scans proving orchd/hostd contain no protocol kinds, adapter
  names, response resource IDs, output/content indexes, continuation-policy
  types, or checkpoint JSON access.
- Full workspace format, clippy, and test gates.

## Alternatives considered

- **Let orchd select full replay versus continuation.** Rejected because it
  makes the agent runtime understand protocol lifecycle and target policy.
- **Store only provider-side state and omit full logical history.** Rejected
  because provider retention, target changes, and checkpoint loss would alter
  durable conversation semantics.
- **Keep `{ adapter, state: JSON }` as the public envelope.** Rejected because
  convention does not prevent branching, construction, schema coupling, or
  accidental logging.
- **Always replay full history.** Rejected because some targets gain material
  continuity, caching, cost, or reasoning behavior from native continuation;
  the gateway can use it without exposing it.
- **Always require a checkpoint for Responses.** Rejected because Responses
  supports stateless use and a protocol name does not determine one state
  policy.
- **Expose one arbitrary provider-options map.** Rejected because it moves
  capability validation into callers and recreates protocol coupling.
- **Make llmd own mutable session state.** Rejected because it conflicts with
  hostd authority and makes restart behavior depend on hidden process memory.

## Rollout

1. Introduce neutral IDs, `ModelRef`, generalized descriptors, typed inference
   options, and capability validation alongside internal adapter contract
   tests.
2. Add the bounded checkpoint codec, target fingerprint, canonical prefix
   digest, and conversation planner; keep adapters private.
3. Migrate `execute` to `start`, replace response-coordinate events, and move
   streaming/non-streaming assembly onto one event contract; add opaque
   durable handles without enabling background execution by default.
4. Migrate orchd, compaction, guardian, and tests to `InferenceRequest`; remove
   `llm_call`, `ItemIdentity`, and flat provider/model request fields.
5. Replace the persisted adapter/state continuation envelope with the opaque
   token carrier and add restart, switch, compaction, and malformed-token
   integration coverage.
6. Remove old public types and compatibility paths, run architecture scans,
   update F-26 acceptance evidence, and add V-38.
