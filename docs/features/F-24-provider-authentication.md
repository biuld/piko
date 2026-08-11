# F-24: Provider authentication

> Status: implemented
> Priority: P0
> Source evidence: OpenAI authentication product documentation and codex-rs
> authentication behavior

## Summary

Users can authenticate a model provider with any method that provider exposes,
including API keys and interactive OAuth. The selected method remains visible
to the runtime so subscription credentials are never treated as metered API
keys. Expiring credentials refresh automatically while the application is in
use, and refreshed credentials remain available after restart.

## Problem

Before F-24, authentication ended at obtaining a bearer string. The runtime
then treated every bearer string as an API key, lost the selected method, and
could not choose a compatible target or preserve account context. Refresh
behavior was selected with provider-name conditionals rather than the
registered provider implementation. That made each new OAuth provider require
coordinated edits throughout the product.

## User journeys

1. A user signs in to OpenAI with an API key. Requests use OpenAI Platform
   access and usage-based billing.
2. A user signs in to OpenAI with ChatGPT. Requests use the ChatGPT subscription
   transport and workspace context rather than the Platform API-key transport.
3. An OAuth access token approaches expiry during an active session. piko
   refreshes it before the next provider request, persists rotated tokens, and
   continues without another interactive login.
4. A provider exposes OAuth support. Every client discovers that capability
   from hostd; no client-side provider allowlist is required.
5. Interactive login is denied, expires, is cancelled, or encounters a
   terminal provider error. The login ends with a typed failure and does not
   block model discovery or unrelated authentication operations.

## In scope

- Distinct API-key and OAuth credential semantics.
- Provider-owned interactive login, refresh, and request-auth materialization.
- Browser-callback interaction for local clients, with device-code login as an
  explicit headless fallback.
- Interactive login cancellation, expiry, and per-provider concurrency.
- Provider authentication capabilities authored by hostd and projected to
  clients.
- Authentication-method-compatible target selection plus provider-owned
  protected request headers.
- Durable storage of refreshed credentials with restricted file permissions.

## Out of scope

- Publishing or promising stability for provider-private OAuth endpoints.
- OAuth client registration for arbitrary third-party applications.
- Cross-device credential synchronization.
- Migrating credentials from other agent products.

## Behavior and states

Authentication methods are `api_key` and `oauth`; clients display only methods
reported by hostd. For a local interactive client, OAuth defaults to a browser
authorization-code flow with PKCE and a loopback callback. A client may request
device-code interaction explicitly when it cannot receive a local callback.
OAuth interaction is correlated by a host-issued `login_id` and moves through
`starting`, `waiting`, and one terminal state: `succeeded`, `denied`, `expired`,
`cancelled`, or `failed`. Waiting is bounded and observes cancellation.

hostd permits at most one active login per provider. It owns the loopback
listener, using only callback ports declared by the provider adapter, validates
callback state, exchanges the code through the provider adapter, and persists
the resulting credential. It never substitutes an arbitrary port when the
provider's registered OAuth client requires an allow-listed redirect. The
client owns the local UI side effect of opening the authorization URL. Failure
to launch a browser does not discard the login: the URL remains visible and
copyable.

An OAuth credential contains an access token, optional refresh token, expiry,
and provider-owned metadata. Before a request, hostd asks the registered
provider authentication implementation to resolve request authentication. A
valid access token is reused. A token in its refresh window is refreshed once,
persisted, and then materialized as method-specific request configuration.
Refresh failure does not silently reuse an expired token.

API-key and OAuth credentials may share a bearer header on the wire, but they
remain different product states because they can select different endpoints,
adapters, headers, billing, and governance.

## Acceptance criteria

- [x] OpenAI API-key credentials use the Platform transport.
- [x] OpenAI ChatGPT credentials use the subscription transport and preserve
  account/workspace request metadata when available.
- [x] OpenAI OAuth credentials refresh before expiry and rotated credentials
  survive restart.
- [x] An expired credential for an unsupported refresh flow fails closed.
- [x] A second OAuth implementation can register login, refresh, and request
  materialization without adding a provider-name branch to shared storage.
- [x] Clients derive OAuth choices from hostd provider capabilities.
- [x] Local OpenAI OAuth opens a browser and completes through a validated
  loopback callback; device code remains an explicit fallback.
- [x] Browser and device-code login expire and can be cancelled.
- [x] Every asynchronous login command receives an immediate correlated start
  response and one typed terminal authentication event.
- [x] Credential files are created with user-only permissions on Unix.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Is an OAuth bearer token an API key? | No | The method controls entitlement, endpoint, billing, and governance. |
| Who owns durable auth state? | hostd | It is user-visible durable configuration. |
| Who owns provider OAuth protocol details? | The llmd provider adapter | Provider protocol differences do not belong in storage or clients. |
| Who owns the loopback callback and login state? | hostd | Login lifecycle and durable auth remain host-authoritative. |
| Who opens the browser? | The local client | Browser launch is a client-machine UI side effect; the authorization URL comes from hostd. |
| Is device code the universal OAuth shape? | No | It is one interactive method and is beta for OpenAI headless login. |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Distinct ChatGPT and API-key login modes | kept | hostd durable auth state and llmd request materialization |
| Refresh before token expiry | kept | provider auth implementation invoked before requests |
| Browser callback as default login | kept (adapted) | hostd owns callback/state; the client opens the URL |
| Device-code login for headless environments | kept | bounded interactive flow |
| OpenAI-specific endpoint and claims in core auth state | rejected | isolated in the OpenAI llmd adapter |

## Open questions

1. Which desktop keyring backends should follow the restricted file backend?

## Reference evidence

- OpenAI authentication documentation: `https://learn.chatgpt.com/docs/auth`
- Existing piko provider registry, OAuth flow, auth storage, host command, and
  gateway request construction.
