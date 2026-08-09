# D-37: Native OpenAI-family model protocols

> Status: implemented
> Implements: [F-25](../features/F-25-native-openai-model-protocols.md)
> Core target modeling amended by ADR-009

> The public gateway, semantic identity, and continuation-envelope portions
> are superseded by
> [D-38](D-38-protocol-neutral-inference.md). This design remains
> authoritative for native adapter and transport behavior.

## First-class target model

The catalog is not a `Provider -> Protocol` map. llmd owns five stable
entities and one resolution result:

```text
Provider -> ApiSurface[]
Provider -> Model[]
Model + ApiSurface -> ModelTargetProfile
ModelTargetProfile + AuthMethod -> ResolvedModelTarget
Credential + frozen AuthMethod -> request headers
```

`ModelKey(provider_id, model_id)` is the only model identity used for target
lookup. `ProtocolProfile` is a closed enum: Responses carries its continuation
policy, while Chat Completions has no Responses fields. A resolved target owns
the target ID, API-surface ID, auth method, base URL/endpoint, and protocol
profile. The executable gateway target is built from that frozen value before
credentials are materialized.

Provider manifests use this non-legacy shape:

```toml
[provider]
id = "example"

[api_surfaces.platform]
base_url = "https://api.example.com/v1"
auth_methods = ["api_key"]

[default_targets.platform]
protocol = "responses"

[models."model-id"]
name = "Model"
# capabilities omitted

[models."model-id".targets.platform]
protocol = "chat_completions" # replaces defaults for this model
```

Catalog validation rejects a missing surface reference and more than one
target compatible with the same auth method. Model-specific target maps
replace, rather than merge with, provider defaults so selection is explicit.
OpenAI declares separate Platform/API-key and Subscription/OAuth surfaces.
DeepSeek declares one API-key surface; its default target is Chat Completions
and `deepseek-v4-flash` replaces that target with stateless Responses.
> Decisions: none; create an ADR before implementation if the semantic/wire
> split becomes a cross-feature public extension point

## Goal

Replace the `genai`-owned request, response, stream, provider-selection, and
error types in `piko-llmd` with a piko-owned semantic gateway and two native
HTTP adapters:

- OpenAI Responses (`POST /v1/responses`);
- OpenAI Chat Completions (`POST /v1/chat/completions`).

The existing orchd-facing gateway remains protocol-neutral. Both adapters
retain enough protocol metadata for correct multi-step reasoning and tool use,
and the existing retry, cancellation, middleware, usage, and cost behavior is
rebased onto piko types.

The correction pass also removes ambiguous compatibility boundaries found by
end-to-end review: protocol-native stream-start state stays inside llmd, each
Responses target profile declares one explicit continuation policy, runtime
targets are resolved independently from credentials, and orchd receives no
transport secret.

The gateway emits one llmd-assembled output metadata value containing the
optional continuation. orchd copies that value into the assistant transcript
message without inspecting it. In particular,
`ProtocolKind`, Responses lifecycle events, response IDs, output-item IDs, and
encrypted reasoning collection remain below the llmd boundary.

## Constraints and non-goals

- `hostd` remains authoritative for settings, model selection, credentials,
  and durable user-visible state.
- `orchd` owns the agent loop and consumes semantic model events only.
- `piko-llmd` owns target resolution, wire encoding/decoding, HTTP/SSE,
  retries, and model-call middleware.
- `piko-protocol` remains a DTO-only shared leaf; wire DTOs stay private to
  `piko-llmd`.
- Existing request cancellation and no-mid-stream-restart guarantees remain.
- Protocol selection is explicit. There is no model-name dispatch and no
  Responses-to-Chat fallback.
- The initial Responses surface is create/stream inference. Resource lifecycle
  operations are not added to the orchd gateway in this design.
- Native Anthropic, Gemini, and other provider protocols are removed from the
  supported runtime surface rather than reimplemented in this migration.

## Proposed design

### Boundary model

Keep `LlmGateway` as the orchd port, but replace the chat-shaped method name
and string errors with protocol-neutral types:

```rust
trait LlmGateway {
    async fn execute(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelEventStream, GatewayError>;

    async fn execute_once(
        &self,
        request: ModelRequest,
        cancel: CancellationToken,
    ) -> Result<ModelResult, GatewayError>;
}
```

`ModelRequest` retains the current session/run/step correlation, resolved
target, frozen prompt, ordered transcript, tool definitions, and thinking
configuration. It adds explicit output/capability intent where the current
gateway would otherwise silently rely on provider defaults.

The semantic output model has two layers:

1. `ModelEvent` contains agent-consumable text, reasoning, refusal, function
   call, usage, completion, and error events, plus one adapter-assembled output
   metadata value.
2. `ModelResult` assembles the same semantic content and output metadata for
   non-streaming execution. The metadata contains the continuation needed to
   construct the next request.

The minimum semantic item set is text, refusal, reasoning, function call, and
function result. Item metadata carries optional response/item/call identity,
indexes, and terminal status. Chat Completions leaves identities it does not
have absent; the gateway never manufactures Responses IDs.

All gateway consumers use `ModelEvent` directly. The previous `GatewayEvent`,
`chat_stream`, semantic-to-legacy conversion, and legacy-to-semantic conversion
are removed rather than retained as a compatibility surface. Every successful
stream must supply output metadata before its terminal event; a consumer that
cannot do so is malformed, not a legacy mode to guess around.

Provider response-start events and native response IDs remain internal to the
selected adapter. The semantic stream exposes an llmd-produced output metadata
value before its terminal event; consumers never infer a protocol from an ID.

### Target resolution

Provider identity is replaced in the execution path by a resolved target:

```rust
struct ModelTarget {
    id: String,
    api_surface: String,
    auth_method: ProviderAuthMethod,
    protocol: ProtocolProfile,
    endpoint: Url,
    model: String,
    headers: HeaderMap,
    capabilities: ModelCapabilities,
    streaming_fallback: bool,
}

enum ProtocolProfile {
    Responses { continuation: ResponsesContinuationPolicy },
    ChatCompletions,
}
```

The persisted catalog may still group targets by provider for product
presentation and auth lookup. Runtime dispatch uses `protocol`, never the
provider ID or model name. URL construction joins a validated base URL with a
fixed operation path unless the target declares an explicit full endpoint.

The executable lookup key is the exact composite `provider/model` model key;
the resolved target identity is `provider/model@api-surface`. The target
configuration type lives in `piko-llmd`, not the shared wire crate or orchd.
Provider identity remains available for authentication and product
presentation but is not an alias in the target registry. A missing target or
protocol fails closed.

Bundled target defaults are:

- OpenAI OAuth / ChatGPT subscription target: Responses, preserving D-36;
- newly created OpenAI Platform targets: Responses;
- API-key targets: use the protocol declared by their catalog target;
- provider catalogs may override protocol and continuation policy per model;
- DeepSeek `deepseek-v4-flash`: Responses with stateless full-history replay;
- other bundled DeepSeek models: Chat Completions until their Responses
  support is documented;
- custom targets: require an explicit supported protocol.

### Authentication boundary

Revise D-36's `ProviderRequestAuth` so it no longer contains a `genai`
`AdapterKind`. Authentication resolution returns only transport material:

```rust
struct RequestAuth {
    headers: HeaderMap,
    expires_at: Option<SystemTime>,
}
```

The target owns protocol and endpoint. The auth adapter owns refresh and
credential-derived headers. Header merging rejects attempts to override
protected transport headers such as authorization, content length, and SSE
accept semantics from an untrusted custom-header source. Secrets are redacted
before diagnostic events or traces are built.

The gateway receives API-key and OAuth material through the same runtime auth
resolver. `OrchdConfig` contains model identity and semantic run settings only;
it does not duplicate API keys, OAuth access tokens, endpoint, protocol, or
transport headers already owned by the constructed gateway.

### Internal module layout

The implementation is split by responsibility, not by marketed provider:

```text
piko-llmd/src/
  gateway/             semantic request, events, result, and error
  target/              catalog resolution and capability validation
  transport/           reqwest client, HTTP errors, SSE framing, cancellation
  protocols/
    responses/         private request/response DTOs and stream state machine
    chat_completions/  private request/response DTOs and stream state machine
  middleware/          semantic pre-request and event/result middleware
  retry/               piko error classification and bounded retry policy
```

Wire DTOs derive `serde` serialization and stay private. The adapters share
HTTP execution and SSE framing but not request or event DTOs. This prevents a
new Responses feature from being forced through a chat message structure.

### Adapter contract

The dispatcher validates target capabilities, then asks one adapter to encode
the request and decode either the JSON response or SSE stream:

```rust
trait ProtocolAdapter {
    fn validate(&self, request: &ModelRequest, target: &ModelTarget)
        -> Result<(), GatewayError>;
    fn encode(&self, request: &ModelRequest, target: &ModelTarget)
        -> Result<HttpRequest, GatewayError>;
    fn decode_response(&self, response: HttpResponse)
        -> Result<ModelResult, GatewayError>;
    fn decode_stream(&self, response: HttpResponse)
        -> Result<ModelEventStream, GatewayError>;
}
```

The actual trait may use associated futures/streams to avoid unnecessary
boxing. It remains internal to `piko-llmd`; it is not a plugin ABI.

### Responses mapping

The Responses adapter uses the item-oriented request and event vocabulary
directly:

- instructions and transcript become ordered input items;
- assistant output, reasoning items, function calls, and function-call
  outputs retain their protocol identity;
- tool definitions and tool choice are encoded from semantic tool intent;
- response-created, output-item, content-part, text/refusal, function-call
  argument, reasoning, usage, incomplete, failed, and completed events feed a
  state machine;
- `previous_response_id` or equivalent continuation is used only when the
  retained state and target policy explicitly select it; transcript replay
  remains available and is never combined accidentally with server-side
  continuation;
- one terminal API event closes the stream; EOF before terminal state is a
  protocol error.

Unknown JSON fields are accepted by serde. Unknown item/event variants are
captured with their type tag for safe diagnostics. They may be ignored only
when the state machine proves they are additive and irrelevant to the
semantic result; otherwise decoding fails explicitly.

Responses continuation is a target-profile policy. Platform targets use
server-side continuation: requests are stored, and when the latest retained
Responses assistant contains a response ID the next request sends
`previous_response_id` plus only the transcript suffix after that assistant.
Without retained continuation, the complete transcript is replayed. ChatGPT
subscription targets use stateless encrypted-reasoning replay: requests set
`store: false`, request `reasoning.encrypted_content`, and replay the retained
opaque reasoning items with the transcript. The two modes are never combined.

Stateless plaintext targets, including DeepSeek Responses, send neither
`previous_response_id` nor unsupported `store`/`include` controls. They replay
the full transcript and encode retained thinking as Responses `reasoning`
items with `reasoning_text` content. This is a third explicit continuation
policy, not a provider-name heuristic. The DeepSeek profile is model-specific
because its published Responses surface currently covers
`deepseek-v4-flash`, not every model in its catalog.

### Chat Completions mapping

The Chat Completions adapter encodes the same request as messages, tools,
tool-choice, reasoning controls supported by the target, and streaming usage
options. Its stream state machine:

- tracks every choice by choice index;
- tracks tool calls by both tool-call index and provider call ID;
- assembles name and argument fragments without assuming one concurrent call;
- preserves refusals and finish reasons;
- emits usage once when reported, including the terminal usage-only chunk;
- rejects a successful EOF without an accepted finish condition.

The agent runtime currently consumes one selected choice. The adapter rejects
or explicitly selects configured multi-choice behavior rather than merging
choices.

### HTTP and SSE transport

Use `reqwest` for HTTP and `serde`/`serde_json` for wire DTOs. Add a small
standards-compliant SSE decoder or a narrowly scoped SSE crate after reviewing
its framing behavior. The transport owns:

- connect, request, and idle timeouts;
- cancellation during send, backoff, and body polling;
- response status, headers, request ID, and bounded error-body capture;
- SSE line framing, multi-line `data`, comments, and `[DONE]` where the
  selected protocol defines it;
- body-size and diagnostic-size limits.

OpenAI's official SDK list currently includes JavaScript/TypeScript, Python,
.NET, Java, Go, and Ruby, but not Rust. Therefore this design does not add a
community Rust OpenAI SDK. Direct typed HTTP adapters minimize a second
abstraction layer and keep both wire contracts under piko's tests.

### Errors, retry, and fallback

`GatewayError` is structured by phase and class:

- target/configuration and unsupported capability;
- authentication;
- transport/connect/timeout;
- upstream HTTP status, request ID, and sanitized API error;
- SSE framing and protocol decoding;
- cancelled.

Retry classification moves from `genai::Error` matching to these types. HTTP
408, 409, 425, 429, selected 5xx responses, connection failures, and timeouts
keep the F-02 bounded policy. `Retry-After` may be honored within the existing
total budget now that response headers are available.

Stream-to-nonstream fallback reuses the same adapter and semantic request. It
is allowed only before an observable semantic delta. Authentication,
validation, and non-retryable protocol failures do not trigger fallback.
Disabling retries disables fallback as required by F-02. A retryable transport
or SSE read failure after HTTP success but before the first observable event is
still in the open phase and may use same-protocol fallback once; malformed JSON,
illegal protocol state, and premature protocol EOF do not.

### Middleware and observability

Middleware receives semantic requests/events and target metadata, never wire
DTOs. Token usage and cost annotation operate on normalized usage while
retaining provider-reported details for diagnostics. Logs include target ID,
protocol, operation, status, request ID, attempt, and stream phase, but exclude
authorization, OAuth tokens, prompt bodies, tool arguments, and raw error
bodies unless an existing explicit debug policy safely permits them.

Cost normalization precedes usage telemetry. Successful JSON and upstream
error bodies are read through byte limits rather than truncated after an
unbounded allocation. Exact prompt snapshots remain the process-local,
bounded, explicitly queried debug surface defined by D-30 and never become log
fields or external telemetry; upstream messages are sanitized before they
become diagnostics.

Output metadata is semantic only in its lifecycle: orchd knows when to persist
it but treats its continuation payload as an opaque durable value. Adapter
stream state owns all protocol-specific accumulation and emits the completed
metadata before the terminal event. This keeps adding a protocol from requiring
an orchd event-lane change.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Evolve shared model content/continuation DTOs only where durable transcript round-tripping requires them; no OpenAI wire DTOs. |
| `piko-hostd` | Parse and merge explicit target protocol; preserve existing target behavior; continue authoritative auth and model selection. |
| `piko-orchd` | Consume richer semantic events and persist the retained continuation/tool identity needed by later steps. |
| `piko-llmd` | Own semantic gateway, target resolution, typed HTTP/SSE transport, both protocol adapters, errors, retry, and middleware; remove `genai`. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Invalid target URLs, missing protocols, conflicting headers, and unsupported
  capabilities fail before network dispatch.
- Non-2xx responses retain status and request ID while bounding and sanitizing
  the error body.
- Malformed JSON/SSE, illegal event order, premature EOF, mismatched tool-call
  fragments, and duplicate terminal events produce protocol errors.
- Cancellation is selected against send, retry sleep, and stream polling. The
  adapter emits no later success after cancellation wins.
- Partial output is never replayed automatically. Recovery remains the
  caller's step-level decision.

## Verification

- Serialization golden tests generated from hand-reviewed OpenAI API fixtures
  for both request formats.
- Decoder/state-machine fixtures for complete and fragmented Responses event
  sequences, concurrent tool calls, reasoning, refusal, incomplete/failed
  responses, usage, malformed ordering, and premature EOF.
- Equivalent Chat Completions fixtures for indexed choices/tool calls,
  argument fragmentation, finish reasons, refusals, terminal usage chunks,
  malformed ordering, and premature EOF.
- Local stub-server integration tests for headers, paths, status errors,
  request IDs, SSE framing, cancellation, retry budget, `Retry-After`, and
  same-protocol non-streaming fallback.
- Contract tests proving middleware observes the same semantic sequence for
  streaming and non-streaming calls.
- Migration tests for OpenAI OAuth Responses targets, existing API-key Chat
  Completions targets, new Responses defaults, and explicit custom protocols.
- Dependency check proving `genai` is absent from `Cargo.toml`, `Cargo.lock`,
  production code, and tests.
- `cargo test --workspace` and
  `cargo clippy --workspace --all-targets -- -D warnings`.

## Alternatives considered

- Keep `genai` and patch missing Responses behavior: rejected because piko
  would still depend on its release cadence, chat-shaped model, adapter
  routing, and error taxonomy.
- Adopt a community Rust OpenAI SDK: rejected for this scope because OpenAI
  does not list an official Rust SDK, and a community SDK would restore the
  same release-cadence and wire-control dependency this design removes.
- Translate Responses into Chat Completions internally: rejected because it
  loses item identity, reasoning/continuation semantics, and newer Responses
  event types.
- Expose both wire DTO models to orchd: rejected because it couples the agent
  loop and transcript logic to HTTP contracts.
- One generic JSON adapter with endpoint templates: rejected because it moves
  correctness into runtime configuration and cannot validate event ordering or
  continuation state.
- Infer protocol from provider/model names: rejected because compatible
  endpoints diverge and model names are not stable protocol declarations.

## Rollout

1. Introduce piko semantic result/error types and explicit `ProtocolKind`
   behind the current gateway; require explicit target protocol selection.
2. Implement shared HTTP/SSE transport and fixture harness.
3. Implement Responses request/response/stream mapping; move the OpenAI OAuth
   target to it and validate reasoning/tool continuity.
4. Implement Chat Completions request/response/stream mapping; migrate existing
   API-key and compatible targets.
5. Move retry, fallback, middleware, token/cost accounting, and telemetry to
   piko-owned types; run both adapter contract suites.
6. Remove all `genai` request/event/error/provider code and the dependency;
   remove unsupported native-provider catalog defaults.
7. Add verification evidence, update F-02's implemented gateway description,
   and mark F-25/D-37 implemented only after both adapters satisfy the PRD.
