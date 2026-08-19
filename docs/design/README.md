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

- [D-55: Time-of-day pricing policy](D-55-time-of-day-pricing-policy.md)
  implements F-39: a registered `time_of_day` policy selects token rates by
  request wall-clock time, with fixed-offset timezone expression, half-open
  windows, and the official DeepSeek V4 peak/off-peak CNY catalog (accepted).
- [D-54: Branch cursor without Leaf nodes](D-54-branch-cursor-without-leaf.md)
  implements F-38: navigate writes only `BranchSelected`; no Leaf wire
  type; current-state cursor is the active branch tip (accepted).
- [D-53: CQRS session read models](D-53-cqrs-session-read-models.md)
  implements F-37: durable catalog, trajectory, and current-state
  projections published on append; list/trajectory/open read those files;
  snapshots and query LRUs are removed (implemented).
- [D-52: Trajectory viewer inline assembly](D-52-trajectory-inline-assembly.md)
  removes the Prompt tab and renders each run's prompt assembly as a
  time-ordered card in the message stream with the same selection/expansion
  behavior as ordinary cards; the timeline prompt marker selects that card
  (implemented; visual QA user-side).
- [D-51: Trajectory prompt assembly and cache debugging view](D-51-trajectory-prompt-and-cache-view.md)
  adds a Prompt tab to the trajectory viewer: the frozen assembly (blocks,
  tool catalog, cache plan) plus per-step provider cache usage, with an
  optional `usage` field on the model-step record (draft).
- [D-50: Trajectory viewer architecture](D-50-trajectory-viewer-architecture.md)
  defines the hostd-served trajectory web viewer: modular static assets
  (no build toolchain), a store with per-slice subscriptions, a canvas
  timeline component, native-scroll DOM lists, loop-free SSE, and CSS tokens
  as the single source of truth (draft).
- [D-49: Agent run trajectory](D-49-agent-run-trajectory.md)
  implements F-36: a durable per-run record (prompt assembly + agent
  trajectory) in the journal as observational event types, served to a
  real-time loopback web viewer over SSE, retiring D-30 and OTel span export
  (draft).
- [D-48: Turn budget headroom and steer responsiveness](D-48-turn-budget-headroom-and-steer-responsiveness.md)
  implements F-35: a bounded output/reasoning reserve in the per-step context
  preflight and a respond-first model step after a steered user message
  (implemented).
- [D-47: Execution denial typing and escalation guidance](D-47-execution-denial-typing-guidance.md)
  implements F-34: type sandboxed OS denials as `sandbox_denied`, derive
  retry roots from the denial text (not a second policy walk), and surface
  escalation guidance. No command-blacklist preflight (implemented).
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
