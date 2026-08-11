# D-36: Provider authentication

> Status: accepted
> Implements: [F-24](../features/F-24-provider-authentication.md)
> Decisions: [ADR-007](../decisions/ADR-007-typed-provider-authentication.md),
> [ADR-011](../decisions/ADR-011-host-owned-oauth-callbacks.md)

## Goal

Deliver typed provider credentials, provider-owned OAuth refresh, dynamic auth
capabilities, and method-specific request configuration without moving durable
authentication authority out of hostd.

## Constraints and non-goals

- OpenAI documents product behavior but not its private device endpoints as a
  third-party protocol contract; those endpoints stay isolated and replaceable.
- Provider secrets never enter model/session transcripts or telemetry.
- piko-protocol remains DTO-only.
- Local OAuth defaults to browser authorization with a loopback callback;
  device code remains an explicit headless fallback.

## Proposed design

### Provider authentication boundary

`OAuthFlow` is a complete provider adapter rather than only a device-login
adapter. It owns browser authorization construction and code exchange,
device-code interaction, refresh, and conversion of an OAuth credential into
`ProviderRequestAuth`. Shared `AuthStorage` owns only durable credential CRUD
and expiry decisions; it contains no provider-name branches.

`ProviderRequestAuth` contains only protected request headers and expiry
metadata. Protocol, endpoint, model, and capabilities remain frozen in the
catalog-selected target per ADR-008/ADR-009. OAuth header materialization is
delegated to the registered OAuth flow.

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
model registry across network waits. hostd stores active operations by provider
and issues a unique `login_id`; a second login for the same provider fails
without replacing the first. Browser callbacks and device polling both select
over expiry and a cancellation token.

### Browser callback

hostd binds a loopback listener before asking the provider adapter to construct
an authorization URL. The adapter declares its registered callback ports;
OpenAI uses `1455` with registered fallback `1457`. hostd tries those ports in
order and fails with device login guidance if neither is available instead of
creating an invalid redirect on an ephemeral port. The adapter generates PKCE
and state, while hostd accepts one bounded callback, validates the returned
state, and delegates code exchange to the adapter. The authorization URL is
emitted as a DTO-only auth event. The TUI opens it with the platform browser and
keeps the URL visible as a copyable fallback. Tokens, authorization codes, PKCE
verifiers, and callback query strings never enter protocol events, transcripts,
or telemetry.

Device code uses the same host-owned operation lifecycle but is selected only
when a client explicitly requests `device_code`.

### Storage

The file backend writes through a same-directory temporary file, restricts
permissions to `0600` on Unix, and atomically renames the completed file.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Authentication-method capability DTOs. |
| `piko-hostd` | Authoritative resolution, callback listener, operation registry, cancellation, and bounded login orchestration. |
| `piko-llmd` | Provider-owned browser/device protocol, refresh/materialization, and typed gateway transport. |
| `piko-tui` | Render host-advertised methods and open browser authorization URLs. |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Expired OAuth without a refresh token or implementation returns a typed
  authentication error; the stale access token is not used.
- Refresh responses that omit a rotated refresh token retain the previous one.
- Login cancellation and provider expiry terminate polling.
- Callback state mismatch and provider denial are typed terminal failures.
- Browser launch failure leaves a visible URL for manual opening.
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
4. Browser-callback default, explicit device-code fallback, and cancellation.
5. Keyring storage backend.
