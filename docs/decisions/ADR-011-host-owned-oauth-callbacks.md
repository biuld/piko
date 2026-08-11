# ADR-011: Keep OAuth callbacks host-owned and browser launch client-local

> Status: accepted
> Date: 2026-08-11

## Context

Local OAuth needs two effects with different authority and locality. The
loopback callback completes a provider protocol and produces durable
credentials, while opening a browser is a UI action on the client's machine.
Putting both in hostd couples the daemon to a desktop environment; putting the
callback or token exchange in TUI makes a transient client authoritative for
authentication state.

## Decision

hostd owns each correlated OAuth login operation, its timeout/cancellation,
the loopback callback listener, callback-state validation, credential
exchange, and persistence. The llmd provider adapter owns authorization URL
construction, PKCE, provider endpoints, registered loopback callback ports,
and token exchange details. hostd binds only a port declared by that adapter;
it does not replace an unavailable registered port with an arbitrary one.

hostd sends the authorization URL to clients as a DTO-only event. A local
client opens that URL with the platform browser and always retains a visible
manual fallback. Browser launch failure does not terminate the host-owned
operation. Device-code login remains an explicit interaction mode for
headless clients.

## Consequences

- Durable authentication authority remains in hostd.
- TUI contains no provider OAuth details or credentials.
- Other clients may present the same challenge differently without changing
  the provider adapter.
- A remote deployment must select device-code mode unless it can route the
  advertised loopback callback to the hostd process.
