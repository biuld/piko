# ADR-007: Preserve typed provider authentication

> Status: accepted
> Date: 2026-08-09

## Context

API keys and OAuth access tokens can both appear as bearer strings, but they
represent different entitlements, endpoints, billing rules, governance, expiry
behavior, and provider metadata. Flattening them to a string made OpenAI
ChatGPT login feed the Platform API-key transport and moved refresh protocol
branches into generic storage.

## Decision

Provider authentication remains typed through request construction. hostd owns
durable credentials and authentication status. llmd provider authentication
adapters own interactive protocol details, refresh exchanges, and conversion
to request endpoint/adapter/headers. Shared storage and clients never select
behavior with provider-name branches.

Private provider endpoints are compatibility details, not piko product
contracts. They remain isolated behind the provider adapter.

## Consequences

- Provider runtime configuration is richer than an `api_key` string.
- Adding an OAuth provider requires one provider adapter registration plus
  catalog metadata, without shared storage or TUI branches.
- Refresh must update hostd-owned durable state before the new credential is
  used.
- Provider-specific transport behavior remains testable without coupling
  orchd to authentication.
