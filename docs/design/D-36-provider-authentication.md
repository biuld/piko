# D-36: Provider authentication

> Status: accepted
> Implements: [F-24](../features/F-24-provider-authentication.md)
> Decisions: [ADR-007](../decisions/ADR-007-typed-provider-authentication.md)

## Goal

Deliver typed provider credentials, provider-owned OAuth refresh, dynamic auth
capabilities, and method-specific request configuration without moving durable
authentication authority out of hostd.

## Constraints and non-goals

- OpenAI documents product behavior but not its private device endpoints as a
  third-party protocol contract; those endpoints stay isolated and replaceable.
- Provider secrets never enter model/session transcripts or telemetry.
- piko-protocol remains DTO-only.
- The first slice keeps the existing device-code interaction and does not add a
  localhost browser callback server.

## Proposed design

### Provider authentication boundary

`OAuthFlow` becomes a complete provider adapter rather than only a login
adapter. It owns interactive login, refresh, and conversion of an OAuth
credential into `ProviderRequestAuth`. Shared `AuthStorage` owns only durable
credential CRUD and expiry decisions; it contains no provider-name branches.

`ProviderRequestAuth` contains the adapter kind, bearer token, endpoint, and
additional headers needed for one provider request. API-key materialization is
derived from the model provider catalog. OAuth materialization is delegated to
the registered OAuth flow.

### Resolution

hostd resolves authentication before constructing the runner and gives llmd a
provider-auth resolver backed by an independent view of hostd's durable auth
file. The gateway invokes that resolver before every streaming or stateless
model request. The resolver serializes concurrent refreshes, persists rotated
credentials, and fails closed instead of returning an expired token. The llmd
gateway preserves auth kind, adapter, endpoint, and headers rather than
flattening them to an API-key string.

### OpenAI adapter

The OpenAI OAuth adapter implements refresh-token exchange, preserves rotated
refresh tokens and ID-token-derived account metadata, and selects the ChatGPT
subscription Responses transport. OpenAI API keys continue to select the
Platform transport from the provider catalog.

### Capabilities

`ProviderInfo` carries serializable authentication methods. `ModelCatalog`
derives OAuth capability from the registry, hostd combines it with API-key
support, and TUI renders the reported methods. This removes the OAuth provider
allowlist from TUI.

### Login concurrency

OAuth flow registrations are shared `Arc` values so hostd does not borrow the
model registry across network waits. Device polling includes expiry and a
cancellation token in its interaction context.

### Storage

The file backend writes through a same-directory temporary file, restricts
permissions to `0600` on Unix, and atomically renames the completed file.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Authentication-method capability DTOs. |
| `piko-hostd` | Authoritative resolution, capability projection, bounded login orchestration. |
| `piko-llmd` | Provider-owned refresh/materialization and typed gateway transport. |
| `piko-tui` | Render host-advertised authentication methods. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Expired OAuth without a refresh token or implementation returns a typed
  authentication error; the stale access token is not used.
- Refresh responses that omit a rotated refresh token retain the previous one.
- Login cancellation and provider expiry terminate polling.
- A failed atomic credential write leaves the previous credential file intact.

## Verification

- Unit tests for expiry, OpenAI refresh payload/response mapping, account claim
  extraction, and request materialization.
- Host integration tests for capability projection and stale-token rejection.
- TUI tests proving menus use host-advertised OAuth methods.
- Existing gateway, auth, and workspace tests remain green.

## Alternatives considered

- Keep `match provider` in `AuthStorage`: rejected because storage would remain
  coupled to every OAuth protocol.
- Treat OAuth access tokens as API keys: rejected because it erases product and
  transport semantics.
- Put refresh in TUI: rejected because clients are not authoritative and may
  disconnect while hostd continues running.

## Rollout

1. Typed credentials, provider refresh, capabilities, and storage hardening.
2. OpenAI subscription request materialization and gateway header support.
3. Per-request refresh resolver.
4. Explicit login cancellation and browser-callback login.
5. Keyring storage backend.
