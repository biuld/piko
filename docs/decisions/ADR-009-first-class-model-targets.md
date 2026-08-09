# ADR-009: Model targets join models, API surfaces, auth routes, and protocols

> Status: accepted
> Date: 2026-08-10
> Supersedes: the catalog-shape and migration-alias portions of ADR-008

## Context

A provider is a product and authentication namespace, not a transport. One
provider may expose multiple API surfaces, and one model may use different
wire protocols on those surfaces. OpenAI Platform and ChatGPT subscription
use different authentication routes and endpoints. DeepSeek exposes models on
one API surface while selecting Responses or Chat Completions per model.

Provider-level `protocol` and `base_url` fields cannot represent both cases
without authentication branches and model overrides that bypass one another.
Looking models up by an unscoped model ID also permits a request to silently
cross provider boundaries.

## Decision

`piko-llmd` owns the following core entities:

- `ModelKey`: the composite `(provider_id, model_id)` identity.
- `ApiSurface`: a named provider API base URL and its accepted auth methods.
- `ProtocolProfile`: a closed protocol variant carrying only its valid
  protocol-specific policies.
- `ModelTargetProfile`: a model-to-API-surface protocol binding.
- `ResolvedModelTarget`: one target selected for a `ModelKey` and auth method.

Provider manifests declare API surfaces, provider default target profiles, and
optional per-model target profiles. A model-specific target set replaces the
provider defaults for that model. Catalog loading rejects unknown surfaces and
ambiguous target sets that offer more than one target for the same auth method.

Hostd selects the active credential method, resolves exactly one compatible
target, and freezes that target in the llmd gateway. Runtime auth resolution
must match the frozen auth method and may only materialize request headers.
There is currently one active durable credential per provider; multi-account
selection is a separate product feature and does not change the target model.

Explicit `(provider, model)` lookup is fail-closed. An absent model never falls
through to another provider. Unscoped model lookup succeeds only when the model
ID is unique across the catalog.

Protocol and continuation-policy types are llmd-owned. Shared transcript DTOs
may carry adapter-produced continuation envelopes, but orchd must not branch on
their protocol variants.

No legacy provider-manifest schema, provider-equals-target lookup, or renamed
authentication alias is retained.

## Consequences

- OpenAI API-key and OAuth routes and DeepSeek per-model protocol selection use
  the same target-resolution algorithm.
- Authentication cannot change a frozen endpoint or protocol at request time.
- Invalid protocol/policy combinations are unrepresentable.
- Model identity cannot silently cross a provider boundary.
- Custom provider manifests must use the first-class API-surface schema.
- Adding multi-account selection later adds credential identity, not another
  provider/protocol routing path.
