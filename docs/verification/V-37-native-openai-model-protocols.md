# V-37: Native OpenAI-family model protocols acceptance evidence

> Date: 2026-08-10
> Verifies: F-25 / D-37
> Environment: macOS; local fixture and stub servers only
> Status: passed

## Reproduction

```bash
cargo fmt --all -- --check
cargo test -p piko-llmd
cargo test -p piko-orchd --lib
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
rg -n "genai" Cargo.toml Cargo.lock packages/llmd packages/orchd packages/hostd packages/protocol
rg -n "ProtocolKind|ModelProtocol|ResponsesReasoningEncrypted|ResponseStarted|ModelContinuation::|EncryptedReasoningItem|output_item_ids|response_id" packages/orchd/src --glob '*.rs'
rg -n '\b(GatewayEvent|GatewayRequest|chat_stream|supports_semantic_execute|ProviderConfig|ModelProviderConfig)\b|Into<ModelEvent>|api: String|api ==' packages/llmd packages/orchd packages/hostd packages/protocol --glob '*.rs'
rg -n "StoredOAuthResolver|AuthStore\\b|from_providers|ProviderTarget|oauth_target|provider_config" packages --glob '*.rs' --glob '*.toml'
```

All commands pass. The dependency, orchd protocol-knowledge, and removed
gateway compatibility scans return no matches.

## Protocol contract evidence

- Responses request fixtures cover frozen instructions, ordered input,
  reasoning controls, function definitions, parallel calls, and function-call
  outputs. Response and stream fixtures preserve response, output-item, call,
  output-index, and content-index identity, along with reasoning, usage, and
  terminal status.
- Chat Completions request fixtures cover system, user, assistant, and tool
  messages. Response and stream fixtures preserve refusals, finish reasons,
  usage-only terminal chunks, and concurrent tool-call fragments indexed by
  call index and ID. Arguments received before an ID are buffered without
  loss. A regression test proves a Chat response ID cannot be reinterpreted as
  Responses continuation state.
- Streaming and non-streaming fixtures for each protocol produce equivalent
  semantic content, usage, and terminal status.
- orchd consumes `ModelEvent`, correlates concurrent calls by semantic call ID,
  and persists llmd-produced continuation state without importing either
  adapter's private wire DTOs or branching on its protocol.
- Each llmd adapter now assembles one `ModelOutputMetadata` value before its
  terminal event. The shared transcript carries only an opaque adapter/state
  continuation envelope. Responses IDs, encrypted reasoning, and Chat tool
  continuation structs are private to llmd; orchd copies the envelope without
  interpreting it. Adding another adapter therefore does not require a
  protocol branch in orchd's event lane.
- The old `GatewayEvent`, `GatewayRequest`, `chat_stream`, conversion adapters,
  missing-metadata default, and `Message::Assistant.api` field are absent.
  Test gateways implement `execute` and emit semantic output metadata just like
  production adapters. A successful terminal without metadata fails closed.
- OpenAI Platform Responses targets use stored server-side continuation and
  send only the transcript suffix with `previous_response_id`. ChatGPT
  subscription targets use `store: false` and replay opaque encrypted
  reasoning. Tests prove the modes are not combined and reasoning is not
  silently discarded.

## Target, auth, and failure evidence

- Provider catalogs declare named API surfaces and model target profiles.
  Target construction selects `/responses` or `/chat/completions` from a
  closed protocol profile. Exact `provider/model` lookup is required; an
  explicit provider mismatch and an unscoped duplicate model ID both fail
  closed. There is no provider alias or missing-protocol/base-URL default.
- Target configuration and capabilities live in llmd. `OrchdConfig` has no
  provider-target map, inline API key, protocol field, or transport headers.
  Catalog TOML uses the exact `responses` or `chat_completions` protocol names;
  old adapter-name aliases are rejected.
- OpenAI API-key and OAuth credentials pass through the same host-owned
  runtime resolver and materialize authorization headers independently of
  target selection. The resolver must match the auth method frozen into the
  target. Raw credentials are absent from orchd configuration.
  Custom headers cannot replace protected auth or transport headers, and
  telemetry snapshots contain the semantic request body rather than
  credentials.
- Production target capabilities are derived from catalog model metadata.
  Target validation rejects unsupported text or reasoning requests before
  dispatch, and an explicit full endpoint is preserved without URL rewriting.
- Local HTTP integration tests exercise retry, cancellation, normalized usage
  and cost middleware, and same-protocol stream-to-JSON fallback for both
  adapters. Retry disablement disables fallback; retryable transport failure
  before observable output may fall back once; partial and protocol-invalid
  streams are never restarted. Pre-output metadata is buffered so fallback
  cannot leak a stale response ID.
- Malformed JSON, invalid SSE UTF-8, unknown required event/item types,
  premature EOF, terminal response-ID mismatch, duplicate output index, and
  duplicate terminal events fail with typed errors. Successful and error
  bodies are bounded, and upstream diagnostics pass through central
  redaction.
- Catalog tests reject unknown API-surface references and target sets with two
  candidates for one authentication method. Host tests prove explicit
  provider/model mismatch, unscoped duplicate model IDs, and frozen-target
  auth-method mismatch all fail closed.
- Middleware receives typed semantic events directly. Cost annotation runs
  before usage telemetry, verified by positive-cost ordering assertions.

## Migration evidence

- `genai` and its transitive runtime code are absent from the workspace lock
  file and model gateway.
- The bundled provider registry advertises only the implemented OpenAI-family
  native surface. Custom catalogs remain available when they explicitly map
  to Responses or Chat Completions.
- Bundled OpenAI targets use separate `platform`/API-key and
  `subscription`/OAuth API surfaces. Both select catalog-declared Responses
  profiles while auth resolution supplies headers only.
- The restored DeepSeek catalog resolves `deepseek-v4-flash` to Responses with
  stateless full-history/plaintext-reasoning replay. Other bundled DeepSeek
  models resolve to Chat Completions. Fixtures verify that DeepSeek's profile
  emits no `previous_response_id`, `store`, or `include`, and accepts
  `reasoning_text` in both complete and streaming responses.
- Protocol implementation files are organized under
  `protocols/responses/` and `protocols/chat_completions/`; cohesive Rust files
  remain below the project's 500-line ceiling.
