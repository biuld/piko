# ADR-008: Separate model targets from authentication material

> Status: accepted
> Date: 2026-08-10
> Supersedes: the endpoint/adapter ownership portion of ADR-007
> Catalog shape and compatibility policy superseded by ADR-009

## Context

API keys and OAuth credentials represent different entitlements, and a product
may offer different compatible targets for those entitlements. Earlier design
put endpoint and adapter selection in request-auth materialization. That made a
credential capable of changing the wire contract after model selection and
created duplicate transport configuration in hostd, llmd, and orchd.

## Decision

Authentication adapters return request headers and expiry metadata only.
Provider catalogs own named model-target profiles containing protocol, endpoint,
capabilities, and fallback policy. Hostd selects one compatible profile while
resolving the session model; llmd freezes it as the executable target before
authentication headers are materialized.

Authentication method may be an input to hostd's product-level target-profile
selection when entitlements require it, but credential contents and auth
adapters cannot author or mutate protocol, endpoint, model, or capabilities.
orchd receives semantic model identity and run settings, not credentials or
transport configuration.

Runtime target lookup uses a target ID. A provider ID remains a separate auth
and presentation identity. ADR-009 removes provider-equals-target lookup and
defines the first-class API-surface catalog.

## Consequences

- OpenAI Platform and ChatGPT subscription transports are catalog target
  profiles rather than branches in credential materialization.
- Adding another authentication method does not add routing logic to storage or
  the gateway.
- Credentials remain refreshable per request without changing the frozen target.
- Catalog and session configuration must reject an auth method for which no
  compatible target profile exists.
- ADR-007 continues to govern typed credentials, refresh, persistence, and
  provider-owned OAuth protocol details; its endpoint/adapter ownership clause
  is superseded by this decision.
