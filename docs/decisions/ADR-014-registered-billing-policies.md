# ADR-014: Use registered billing adapters and policies

> Status: accepted
> Date: 2026-08-12

## Context

Catalog-owned prices solve provider/model selection but not varied charging
semantics. A central token calculator would accumulate provider branches and a
closed ledger would force schema changes for every new unit.

## Decision

`piko-llmd` exposes an in-process `BillingRegistry` with independently named
`UsageAdapter` and `PricingPolicy` contracts. Targets select both by ID and
carry opaque policy configuration. Shared usage and cost DTOs use normalized,
open component names. The executable explicitly registers trusted plugin code;
piko does not load arbitrary billing binaries.

## Consequences

- Adding a provider policy does not change the generic middleware.
- Provider manifests remain data and are validated against a registry.
- Stable string identifiers become compatibility contracts and require tests.
- Dynamic third-party code loading remains a separate security/product choice.
