# V-38: Protocol-neutral inference acceptance evidence

> Date: 2026-08-10
> Verifies: F-26 / D-38
> Environment: macOS; local fixtures and stub servers only
> Status: passed

## Reproduction

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p piko-llmd --test protocol_neutral_boundary
cargo test -p piko-llmd --test conversation_identity
cargo test -p piko-llmd --test durable_execution_contract
rg -n "llm_call|ModelRequest|ModelResult|ModelContinuation" \
  packages/*/src --glob '*.rs'
rg -n "previous_response_id|encrypted_content|AdapterItemIdentity" \
  packages/hostd packages/orchd packages/protocol --glob '*.rs'
```

All format, test, and lint gates pass. The removed-abstraction and consumer
protocol-leakage scans return no matches; the llmd-private adapter identity and
Responses continuation fields remain confined to llmd.

## Unified boundary and replay evidence

- `InferenceRequest` carries a provider-qualified `ModelRef`, complete logical
  `Conversation`, typed tools and options, and an invocation context. orchd,
  compaction, and guardian all call `InferenceGateway::start`; the former
  `llm_call`, `ModelRequest`, `ModelResult`, and adapter-tagged continuation
  surfaces are absent.
- Chat Completions and Responses consume the same request. Chat always plans a
  full replay. Responses continuation tests prove that a valid checkpoint
  sends only the suffix after its assistant carrier and that stateless targets
  still replay the complete conversation.
- The checkpoint planner validates a bounded versioned envelope, target
  fingerprint, anchor adjacency, canonical prefix digest, and assistant
  carrier digest. Tests cover malformed data, unknown versions, provider,
  model, API-surface, protocol-profile, and continuation-policy changes,
  modified history or assistant output, removed compaction anchors, and
  newest-compatible selection. Replay-safe targets fall back to full replay;
  replay-required targets return `ContinuationUnavailable` before dispatch.
- Checkpoints have redacted debug output and no public payload accessor.
  Protocol deserialization enforces the persisted-token size bound. Prompt
  diagnostics record semantic input without checkpoint contents, response
  IDs, encrypted reasoning, credentials, endpoints, or protocol fields.
- The host session-storage integration fixture persists an assistant
  checkpoint, reopens the session, and restores the exact opaque token without
  decoding or rewriting it.

## Semantic output, capabilities, and ownership evidence

- Output item IDs are piko hashes derived from invocation identity and semantic
  ordinals. Conversation identity tests prove provider names, timestamps, and
  checkpoints cannot change canonical item IDs, while duplicate semantic
  items remain separately addressable. Provider response/item coordinates and
  stream indexes stay adapter-private.
- Target validation rejects unsupported modality, reasoning effort, tool
  locus, tool choice, parallel-call, structured-output, delivery, and output
  limit intent before transport. Provider reasoning strings are catalog data
  private to llmd; public model summaries expose closed semantic capability
  values and no endpoint, auth, protocol, or continuation fields.
- Chat and Responses complete/stream fixtures collect to equivalent semantic
  items, usage, finish reason, and checkpoint. OpenAI API-key/OAuth and the
  bundled DeepSeek Chat/Responses profiles enter through the same target and
  inference contracts.
- Caller, upstream, and hybrid tools have typed capability and request forms.
  Upstream activity, approval requirements, sources, citations, and artifacts
  are semantic events and durable protocol blocks, without raw provider tool
  JSON.
- orchd's upstream-event test establishes the critical ownership boundary: it
  projects upstream observations into assistant output but neither authorizes
  nor executes them. Catalog capability is insufficient for dispatch; upstream
  execution additionally requires explicit authorization and an adapter gate.
  Production target construction leaves that gate disabled, so no upstream
  external action is enabled by this slice. A future enabling path must obtain
  hostd-owned policy approval first.
- Provider-side compaction is represented only as target state capability and
  opaque planning state. It cannot mutate the logical conversation supplied
  by hostd/orchd; removing its anchored prefix merely makes a checkpoint
  ineligible.

## Durable lifecycle and architecture evidence

- Opaque execution handles and event cursors are size-bounded and redacted.
  The durable contract fixture detaches by dropping a stream, serializes and
  restores the handle, attaches from the beginning, resumes after a cursor
  without duplicate events, observes terminal state, and cancels.
- Durable/background behavior is capability-conditional. No production target
  advertises or enables it in this slice; the neutral lifecycle is contract
  tested without inventing a provider resource DTO.
- Architecture scans recursively inspect model-call consumers and public
  gateway/protocol DTOs. They reject protocol profiles, continuation policies,
  Responses wire fields, adapter identities, adapter-tagged checkpoints, and
  checkpoint JSON access outside llmd.
- The assembled collector permits one completed checkpoint before the terminal
  event and rejects duplicate, post-terminal, or incomplete checkpoint state.
  orchd clears pending checkpoint state on error, cancellation, or incomplete
  completion, preserving hostd's durable transcript authority.
