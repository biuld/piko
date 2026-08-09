# F-25: Native OpenAI-family model protocols

> Status: implemented (D-37)
> Priority: P0
> Source evidence: piko product direction; OpenAI Responses and Chat
> Completions API contracts;
> [DeepSeek Responses API contract](https://api-docs.deepseek.com/api/create-response)

> Public inference semantics, identity, and continuation exposure are refined
> by [F-26](F-26-protocol-neutral-inference.md). This feature remains
> authoritative for native wire-adapter behavior.

## Summary

piko owns the model-call contract used by its agent runtime and supports two
explicit OpenAI-family wire protocols: the Responses API and the Chat
Completions API. A configured model target selects one protocol; piko maps
both protocols into one semantic request and event contract without silently
discarding protocol-specific state needed for later turns. Other native model
protocols are not part of this feature.

## Problem

The current model gateway delegates its wire model and provider selection to a
general-purpose model library. That creates three product problems:

1. piko cannot control when new Responses fields, item types, or stream events
   become usable.
2. Provider differences are hidden behind a lowest-common-denominator chat
   model, so information needed for reasoning and tool-call continuity can be
   lost before it reaches the agent runtime.
3. Provider names, authentication methods, endpoints, and wire protocols are
   coupled even though they are separate choices.

piko currently needs only the OpenAI-family inference surface. Supporting many
native provider protocols through one third-party abstraction adds complexity
without serving the current product scope.

## User journeys

1. A user configures an OpenAI Platform model with the Responses protocol.
   piko streams text, reasoning, tool calls, usage, and terminal status while
   preserving the response and item identity needed for a later model step.
2. A user configures an OpenAI-compatible service that implements Chat
   Completions. piko sends chat messages and tools to that endpoint and emits
   the same semantic agent events, including incremental tool-call arguments
   and usage when the service reports it.
3. An operator explicitly selects the protocol for a model target. piko never
   guesses the protocol from the model name or silently falls back to the
   other protocol.
4. A service claims OpenAI compatibility but returns an unsupported or
   malformed payload. piko reports a protocol error that identifies the target
   and operation; it does not reinterpret the payload as another protocol.
5. A user authenticates with an OpenAI API key or the supported OpenAI OAuth
   flow. Authentication supplies request credentials while the selected model
   target independently determines the endpoint and wire protocol.

## In scope

- One piko-owned semantic model-call contract for prompts, transcript input,
  tools, reasoning controls, streaming output, usage, terminal status, and
  cancellation.
- Native support for exactly two wire protocols:
  - OpenAI Responses create, both streaming and non-streaming;
  - OpenAI Chat Completions create, both streaming and non-streaming.
- Protocol-native handling of text, reasoning, function tool calls and
  results, usage, completion status, API errors, and stream termination.
- Preservation of protocol state that affects correctness on later steps,
  including response, output-item, and tool-call identity when supplied.
- Explicit model-target configuration for protocol, endpoint, model, headers,
  authentication method, and declared capabilities.
- OpenAI Platform and other endpoints that conform to one selected protocol.
- Existing bounded retry, fallback, cancellation, middleware, token usage, and
  cost behavior after it is moved onto the piko-owned contract.
- Removal of the general-purpose model library after both native adapters meet
  this feature's acceptance criteria.

## Out of scope

- Native Anthropic Messages, Google Gemini, Cohere, or other non-OpenAI wire
  protocols.
- Automatically claiming compatibility with every service that describes
  itself as OpenAI-compatible.
- Guessing a protocol from a provider name or model identifier.
- Automatically translating a failed Responses request into Chat Completions,
  or the reverse.
- OpenAI APIs unrelated to model inference, such as fine-tuning, files,
  batches, administration, Realtime, images, audio, and video.
- Exposing the full Responses resource-management surface (retrieve, delete,
  cancel, background execution, and input-item listing) before an agent-runtime
  consumer requires it. The create operation must remain extensible to those
  operations.
- A public provider-plugin ABI.

## Behavior and states

### Protocol selection

Every executable model target has one protocol: `responses` or
`chat_completions`. Selection is explicit and stable for the duration of a
request. Authentication resolution may refresh credentials, but it cannot
change the selected protocol or model.

Bundled OpenAI Platform targets may provide product defaults. Custom targets
must declare their protocol. Model names and endpoint shapes are not used as
runtime heuristics.

### Common semantics without information loss

The common contract represents the concepts the agent runtime consumes:

- instructions and ordered conversation input;
- multimodal content supported by the selected target;
- tool definitions, tool choice, tool calls, and tool results;
- reasoning controls and reasoning output;
- text and refusal output;
- usage, completion status, errors, and cancellation.

Protocol adapters may attach typed protocol metadata to semantic items. A
mapping must fail with an unsupported-capability error when a requested
semantic feature cannot be expressed by the selected protocol. It must never
silently omit the feature.

Protocol selection and continuation assembly remain inside `piko-llmd`.
`orchd` may carry the resulting durable continuation value into an assistant
message, but it must not inspect a protocol kind, interpret provider response
identities, collect protocol-private state, or construct a protocol-specific
continuation variant.

### Responses behavior

The Responses adapter preserves the API's item-oriented model. Response IDs,
output-item IDs, call IDs, output indexes, reasoning state, incomplete status,
and terminal error information remain available through the semantic result
or retained continuation state. Stream events are accepted in legal order and
produce exactly one terminal outcome.

Unknown additive fields are tolerated. An unknown event or item type that is
required to construct the agent result fails explicitly and includes a safe
diagnostic; it is not coerced into text.

### Chat Completions behavior

The Chat Completions adapter preserves ordered messages, assistant tool calls,
tool-call IDs, indexed streaming choices, finish reasons, refusals, and usage.
Incremental arguments for concurrent tool calls remain associated with the
correct call index and ID.

Chat Completions does not gain synthetic Responses identities. Features that
exist only in Responses are reported as unsupported unless the configured
target declares a compatible extension that piko intentionally supports.

### Failure, fallback, and cancellation

Transport, authentication, rate-limit, protocol-decode, unsupported-feature,
and upstream API failures are distinguishable. Retry and stream-to-nonstream
fallback retain the bounded F-02 behavior, but fallback stays within the same
wire protocol. No request is restarted after observable output has been
delivered.

Cancellation aborts connection setup, backoff, and stream consumption. A
cancelled request has one terminal cancelled outcome and cannot later emit a
successful completion.

## Acceptance criteria

- [x] Provider catalogs model named API surfaces separately from providers,
      models, credentials, and wire protocols.
- [x] A model target is selected from `(provider, model, auth method)`; explicit
      provider/model lookup fails closed and unscoped duplicate model IDs are
      rejected as ambiguous.
- [x] Protocol profiles are a closed llmd-owned type, so a Chat Completions
      target cannot carry a Responses continuation policy.
- [x] Runtime credential resolution must match the auth method frozen into the
      selected target and can only contribute request headers.
- [x] OpenAI API-key/OAuth routing and DeepSeek per-model protocol routing use
      the same API-surface/target selection algorithm.

- [x] A Responses fixture covering instructions, input items, reasoning,
      parallel function calls, tool outputs, usage, and completion round-trips
      through the semantic contract without losing required identity.
- [x] Streaming and non-streaming Responses calls yield equivalent semantic
      completed results for the same fixture.
- [x] A Chat Completions fixture covering system/user/assistant/tool messages,
      concurrent indexed tool calls, refusals, finish reasons, and usage
      round-trips through the semantic contract.
- [x] Streaming and non-streaming Chat Completions calls yield equivalent
      semantic completed results for the same fixture.
- [x] A target explicitly selects one protocol; no provider/model-name
      heuristic or cross-protocol fallback is present.
- [x] Unsupported semantic features fail before dispatch when capability
      information is available, and otherwise fail as typed protocol errors;
      they are never silently dropped.
- [x] API-key and OpenAI OAuth credentials materialize request headers without
      owning protocol selection.
- [x] Retry, cancellation, usage/cost middleware, redaction, and streaming
      fallback pass for both adapters and do not depend on third-party error or
      event types.
- [x] Malformed JSON, malformed SSE, unknown required item/event types,
      premature EOF, and duplicate terminal events fail deterministically.
- [x] The model gateway and its tests contain no runtime dependency on the
      general-purpose model library.
- [x] Existing hostd/orchd model-gateway consumers use the common semantic
      contract and do not import either wire protocol's DTOs.
- [x] A streamed Chat Completions assistant turn is persisted as Chat
      Completions state and can be followed by another Chat turn without a
      cross-protocol continuation error.
- [x] Responses continuation is consumed on the next request according to one
      explicit target policy; server-side continuation never duplicates
      replayed transcript input. Stateless targets explicitly choose either
      opaque encrypted-reasoning replay or full transcript replay with
      provider-supported plaintext reasoning items.
- [x] A provider catalog may select different explicit protocols per model.
      DeepSeek `deepseek-v4-flash` uses stateless Responses replay, while its
      models not documented for Responses remain Chat Completions targets.
- [x] Hostd may use the selected auth method to choose a compatible catalog
      target, but authentication material changes request headers only and
      cannot mutate the frozen protocol, endpoint, model, or capabilities.
      Raw credentials are not copied into orchd configuration.
- [x] Production targets derive declared capabilities from catalog metadata;
      retry disablement also disables fallback.
- [x] Retryable stream-open failures before the first observable semantic
      delta may use same-protocol fallback, while observable streams and
      protocol failures are never restarted.
- [x] Semantic middleware retains typed event identity, cost is calculated
      before usage telemetry is emitted, and captured diagnostics are bounded
      and redacted.
- [x] orchd persists llmd-produced output metadata without importing or
      branching on a model protocol kind, Responses-only event, response ID,
      output-item ID, or encrypted reasoning representation.
- [x] The gateway exposes only the semantic `execute`/`execute_once` surface;
      no legacy event enum, chat-shaped stream method, protocol default, or
      missing-metadata fallback remains.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Which model protocols does piko support now? | OpenAI Responses and OpenAI Chat Completions only | These are the current product's essential inference interfaces; narrower scope permits protocol-faithful support |
| Is an OpenAI-compatible service a new provider adapter? | No; it is a target configured against one supported protocol | Provider identity, endpoint, auth, and wire format are independent axes |
| Is Responses translated through a chat abstraction? | No | Its item identities, reasoning, status, and continuation semantics must remain available |
| Is Chat Completions deprecated inside piko? | No; it remains a first-class supported protocol | Many interoperable services expose it, while Responses is the preferred OpenAI API for new work |
| May piko silently omit unsupported request fields? | No | Silent degradation makes agent behavior and tool execution unpredictable |
| Does native Responses support imply every Responses resource endpoint? | No; this feature owns create/stream inference and leaves resource lifecycle operations demand-driven | The agent runtime currently consumes model execution, not general Responses resource management |

## Deferred questions

- A future custom-target escape hatch for additive request fields remains
  deferred. If introduced, it must be protocol-namespaced, redactable, and
  unable to override piko-owned identity, auth, or streaming controls.

## Reference evidence

- OpenAI Responses API guide and API reference:
  <https://developers.openai.com/api/docs/guides/migrate-to-responses>
- OpenAI Chat Completions create reference:
  <https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create>
- OpenAI official SDK list:
  <https://developers.openai.com/api/docs/libraries>
- piko F-02 model-gateway behavior, whose transport-independent retry,
  fallback, cancellation, and usage requirements remain in force.
