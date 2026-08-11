# ADR-012: Keep executable model catalogs locally authoritative

> Status: accepted
> Date: 2026-08-12

## Context

Provider model-list APIs answer different questions with incompatible schemas.
OpenAI's public `GET /v1/models` returns model identity and ownership but does
not declare whether a model uses Responses, Chat Completions, embeddings,
images, audio, or another protocol. The private Codex account catalog is richer
but is not a stable cross-provider contract and still does not own piko's
endpoint or protocol selection.

piko must know the API surface, authentication compatibility, wire protocol,
continuation policy, context/output limits, modalities, reasoning mapping, and
tool capabilities before it can form a safe executable target. Those facts
remain necessary even when a remote catalog is available.

A generic discovery subsystem would add provider-specific transports, cache
partitioning, freshness, last-known-good persistence, source merging, and
failure states while still depending on the local catalog for execution.

## Decision

The locally bundled or explicitly configured provider catalog is the sole
authority for executable models and targets. It owns:

- provider and model identity;
- authentication-method compatibility;
- API surface, endpoint, protocol, and continuation policy;
- semantic capabilities and provider-private wire mappings; and
- context, output, modality, reasoning, and tool constraints.

piko will not implement generic remote model discovery or use a remote model
list to create executable targets. Login success lists the local models valid
for the selected provider and authentication route. Unknown remote IDs do not
enter the picker automatically. Users add custom providers or models through
explicit local configuration.

A future provider may implement optional account-availability reconciliation
only when it has a stable, useful account catalog and a concrete product need.
Such reconciliation may hide or mark locally known models, but it cannot add a
target, change an endpoint/protocol, or widen a local capability. It requires a
new feature PRD and implementation design; it is not reserved by this ADR.

## Consequences

- Model listing is deterministic, offline, and independent of catalog network
  failures.
- Provider and auth-specific model contracts remain reviewable and testable in
  source control.
- New managed models require a piko catalog update; custom models remain an
  explicit user configuration path.
- The picker may temporarily show a locally supported model that an account
  cannot access. The provider's typed authorization/model-unavailable error is
  surfaced instead of silently switching models.
- piko avoids treating endpoint-path similarity such as `/v1/models` as schema
  or execution compatibility.
- ADR-008 and ADR-009 remain authoritative for authentication and target
  resolution.

## Rejected alternative

Remote-first catalog merging with last-known-good caching was rejected because
the available remote schemas cannot replace local execution contracts, while
the required cache and authority machinery is substantial. It may be revisited
only for the narrower availability use case described above.
