# F-29: Provider-pluggable billing

> Status: reviewed
> Priority: P1
> Source evidence: piko provider-native cost accounting and product direction

## Summary

piko estimates model cost through provider-selectable usage adapters and pricing
policies. A provider can introduce new billable units and pricing behavior
without changing the gateway's generic cost middleware or the durable ledger.

## Problem

F-28 moved prices into provider catalogs, but its calculator still understands
only one token schedule. New provider products may charge for requests, images,
audio duration, tools, cache lifetime, time windows, or provider-specific
tiers. Adding those rules to one central calculator would couple every provider
and repeatedly change shared session data.

## User journeys

1. A provider target resolves with a billing plan selected by model and API
   surface.
2. Its usage adapter converts semantic usage into named billable units.
3. Its pricing policy returns native-currency ledger components; the session
   and clients continue to display the accumulated total.

## In scope

- A registry for named usage adapters and pricing policies in `piko-llmd`.
- Open, normalized billable-unit names and open cost-component names.
- Catalog selection of adapter and policy per model/API surface.
- Migration of current OpenAI and DeepSeek token schedules to the standard
  registered policy.
- Programmatic registration for future provider implementations.

## Out of scope

- Dynamic native-library or WebAssembly plugin loading.
- Live provider price discovery, invoices, credits, taxes, or account balances.
- Adding non-token prices that no current provider catalog declares.

## Behavior and states

- Unknown adapters, policies, invalid configuration, or invalid emitted values
  reject catalog/target configuration rather than silently estimating zero.
- Missing billing plans remain valid and produce an empty cost ledger.
- A policy may emit multiple currency/basis entries. Session accumulation merges
  entries only when both labels match and merges components by name.
- Policy failures do not abort model output; they leave cost unavailable and
  produce diagnostic telemetry.

## Acceptance criteria

- [x] Generic cost middleware contains no provider or model price branches.
- [x] Standard token pricing is implemented behind registered adapter/policy
  interfaces.
- [x] Provider/model/surface catalogs select a billing plan.
- [x] Billable units and ledger components accept new names without DTO changes.
- [x] OpenAI USD/API-equivalent and DeepSeek CNY behavior remains unchanged.
- [x] Tests prove custom adapters and policies can be registered without editing
  the middleware.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| What does pluggable mean? | In-process registry contracts, not downloadable code | Keeps trust and deployment explicit while removing core branching |
| Where are prices selected? | Local provider catalog by model and API surface | Preserves ADR-012 local authority |
| What crosses durable boundaries? | Named units and named monetary components | Future unit types do not require another session schema redesign |
| What happens on estimation failure? | Output succeeds; cost is unavailable and diagnosed | Estimates must not break inference |

## Open questions

None for this slice.

## Reference evidence

- [F-28 provider-native cost accounting](F-28-provider-native-cost-accounting.md)
- [ADR-012 local model catalog authority](../decisions/ADR-012-local-model-catalog-authority.md)
