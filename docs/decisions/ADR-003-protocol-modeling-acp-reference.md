# ADR-003: piko host–client protocol is product-owned; ACP is a modeling reference

> Status: accepted
> Date: 2026-08-08

## Context

piko clients (TUI, GUI) talk to **hostd** over a JSON-lines command/event
protocol in `piko-protocol`. That surface is still early relative to the
runtime (F-01 turn lifecycle, multi-agent, session trees, compaction, usage
ledgers). Gaps show up as incomplete client chrome (for example context
`used` without `size`) and state machines that are specified in code paths more
than in a single client-facing contract.

The Agent Client Protocol (ACP) standardizes **editor ↔ coding-agent**
interactions. ACP v1/v2 document mature shapes for:

- session lifecycle and prompt admission,
- streaming message/tool/plan updates with stable ids,
- foreground work state (`running` / `idle` / `requires_action` in v2),
- structured permissions, usage (`used`/`size`/cost), and extensibility.

Replacing the piko host–client protocol with ACP would optimize for third-party
editor clients, but ACP's standard surface does not model piko's product
domain: host-authoritative session trees, multi-agent AgentInstance graphs,
queue/steer semantics beyond generic beyond-turn hooks, prompt/skills catalog
surfaces, and host-owned sandbox execution. ACP v2 is also a draft and
intentionally leaves steering/queueing as separate concerns.

This is analogous to codex-rs under ADR-002: a high-quality external design,
not a specification to port 1:1.

## Decision

1. **`piko-protocol` remains the product host–client contract.** hostd stays
   authoritative for durable user-visible state; clients are projectors and
   intent senders. First-party TUI/GUI do **not** switch to ACP as their sole
   wire protocol.

2. **ACP is a modeling reference for client-visible agent behavior**, in the
   same spirit as ADR-002 for codex-rs:
   - Distill useful turn/stream/tool/usage/permission semantics into Feature
     PRDs (starting with F-22) and designs.
   - Adopt shapes that improve clarity where they fit host-authoritative
     layering.
   - Do **not** translate ACP client filesystem/terminal ownership (where
     clients execute tools) into piko; host/sandbox remains the execution
     authority.
   - Do **not** collapse piko multi-agent, session tree, config namespaces, or
     host snapshot/reconcile models into ACP flat-session assumptions.

3. **Conflicts** (ACP vs piko layering, or ACP vs existing F-01/F-09/F-10
   contracts) resolve by **user discussion + industry best practice**, keeping
   what is best for piko. Unclear points pause; they must not be decided by
   silent ports.

4. **Optional future ACP adapter** may project a subset of host state as an
   ACP Agent for third-party editors. That adapter is a product surface of its
   own; it must not drive the internal protocol redesign. Standard ACP clients
   will see only the mapped subset plus any explicitly versioned extensions.

5. **Wire evolution is PRD-first** (ADR-001): behavior contracts before
   schema churn; implementations record fusion decisions against ACP
   (kept / kept (adapted) / rejected) in the relevant PRD or design.

## Consequences

- Client protocol work prioritizes a coherent **projection lifecycle** (F-22 /
  D-34) over rewrite-as-ACP.
- ACP-aligned improvements (stable stream item ids, foreground state,
  `used`/`size` usage, permission subject shapes) land when PRDs accept them.
- ACP-specific identities (Agent vs Client execution split, JSON-RPC method
  names, pure session modes APIs) are not defaults.
- Ecosystem multi-client remains a deliberate later slice (adapter), not the
  justification for gutting host-domain types.
- codex-rs remains a runtime modeling reference (ADR-002); ACP complements it
  for **client-visible interaction semantics**, not orchd internals.
