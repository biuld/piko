# Implementation designs

Design docs describe how a feature is built: data flow between packages,
responsibility boundaries, protocol types, state ownership, and key technical
decisions. Write a design doc after the feature PRD is agreed, before
implementing.

Create new documents from [`_TEMPLATE.md`](_TEMPLATE.md). Use stable
identifiers. A design's number matches the feature it implements: the F-06
tool-system design is `D-06-tool-dispatch.md`, and the F-01 turn-runtime
design is `D-01-turn-runtime.md`. Feature-local choices belong in the design;
decisions that affect multiple features or package boundaries belong in
[`../decisions/`](../decisions/).

## Recent designs

- [D-46: OTel GenAI prompt inspection](D-46-otel-genai-prompt-inspection.md)
  implements the first two stages of a vendor-neutral LangSmith-like trace
  view with safe assembly metadata by default and separately opted-in GenAI
  model-input export.
- [D-45: Local installer and filesystem configuration](D-45-local-installer.md)
  implements F-33 with binaries under `~/.piko/bin`, idempotent config
  initialization, and installed files as runtime catalog authority.
- [D-44: Session bookkeeping](D-44-session-bookkeeping.md)
  implements F-32 with a hostd domain ledger for incurred usage/cost and
  F-04 occupancy consumed by compaction (implemented; V-43).
- [D-43: Event-sourced session store](D-43-event-sourced-session-store.md)
  implements schema-v4 with a dedicated `piko-session-store` crate, one
  host-owned journal, deterministic reducers, durable accounting, explicit
  branch ancestry, and compatibility/upcasting rules (implemented; V-42).
- [D-42: Per-agent usage projection and TUI](D-42-per-agent-usage.md)
  implements F-30 with durable per-AgentInstance usage/time aggregation and a
  host-refreshed `/usage` surface (implemented; V-41).
- [D-41: Provider-pluggable billing](D-41-provider-pluggable-billing.md)
  implements F-29 with registered usage adapters, pricing policies, and open
  billable/cost components (implemented; V-40).
- [D-40: Provider-native cost accounting](D-40-provider-native-cost-accounting.md)
  implements F-28 with catalog-owned API-surface prices, typed estimate bases,
  native currencies, and a multi-entry session ledger (superseded by D-41;
  V-39).
- [D-38: Protocol-neutral inference boundary](D-38-protocol-neutral-inference.md)
  implements F-26 and hides full-replay versus continuation planning behind a
  general semantic request, event, capability, and opaque-checkpoint model
  (implemented; V-38).
- [D-37: Native OpenAI-family model protocols](D-37-native-openai-model-protocols.md)
  implements F-25 and replaces genai with piko-owned Responses and Chat
  Completions adapters (implemented; V-37).
- [D-36: Provider authentication](D-36-provider-authentication.md) implements
  F-24 typed provider authentication and refresh.
