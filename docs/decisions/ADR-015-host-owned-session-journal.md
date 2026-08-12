# ADR-015: Use a host-owned canonical session journal

> Status: accepted
> Date: 2026-08-12

## Context

Schema-v3 persists mutable session metadata in `session.json`, transcript facts
in per-AgentInstance JSONL shards, and live projections in hostd memory. A
logical operation may update only some of those representations. The storage
adapter also lives inside `piko-hostd`, which makes filesystem mechanics,
schema evolution, replay policy, accounting projection, and application
orchestration difficult to test or evolve independently.

The new persistence contract must support append-only history, message/tree
branching, deterministic live/replay convergence, exact usage attribution,
safe crash recovery, and future compatible event evolution. Hostd must remain
authoritative for user-visible state, while orchd must remain the owner of the
agent runtime and must not write host storage directly.

## Decision

- Each new-generation session has one canonical, ordered, append-only event
  journal. Mutable manifests, per-agent shards, snapshots, and indexes are not
  independent facts.
- Hostd is the sole commit authority. Orchd submits commit intents through
  ports and acts on durable acknowledgement; it never opens or appends the
  journal.
- A new workspace crate, `piko-session-store`, owns the durable schema,
  journal I/O, integrity and tail recovery, snapshots, event upcasting,
  deterministic session reducer, and accounting projections.
- `piko-session-store` depends only on lower-level shared DTOs such as
  `piko-protocol`; it does not depend on `piko-hostd`, `piko-orchd`, or the TUI.
- `piko-hostd` owns use-case orchestration, path/configuration resolution,
  authorization, runtime attachment, and client projection. Its storage
  adapter maps application commands and orchd commit intents to durable session
  events and maps the reduced aggregate to host-facing views.
- Live durable state and recovered state use the same reducer. A durable event
  is appended and synchronized before it is applied to live state or published
  as committed client output.
- Usage and provider-native costs are immutable attributed events, not values
  inferred solely from the current transcript. Corrections append new events.
- Event envelopes and event payloads have independent versions. Unknown
  required events fail with an upgrade-required error; declared optional
  events and namespaced extensions may be preserved or ignored as specified.
- The schema-v4 cutover removes the schema-v3 reader, writer, persisted DTOs,
  compatibility branches, and generation-specific tests. There is no v3
  migration/import path and no dual-write period. Existing v3 directories are
  left untouched on disk but are unsupported and undiscoverable after cutover.

## Consequences

- Session durability and accounting invariants have one implementation and a
  dedicated fault-injection test surface.
- Hostd stays authoritative without retaining filesystem and schema mechanics
  in its application/domain modules.
- Orchd/hostd boundaries remain explicit: runtime facts flow through commit
  ports, and durable acknowledgements flow back.
- New session events become long-lived compatibility contracts and require
  versioning, fixtures, upcasters, and unknown-event policy.
- Rebuildable snapshots and indexes may improve performance but cannot be
  treated as authority.
- The cutover deliberately gives up opening existing schema-v3 sessions and
  simplifies every production path to one storage generation.
- A generic event-journal crate is not introduced now. The low-level journal
  module may be extracted later only when another real bounded context needs
  the same mechanics.
