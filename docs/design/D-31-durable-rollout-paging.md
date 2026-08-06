# D-31: Durable rollout paging

> Status: accepted
> Implements: [F-15](../features/F-15-observability.md)

## Goal

Expose bounded inspection of piko's existing per-AgentInstance append-only
JSONL transcript without adding a parallel rollout recorder.

## Design

`RolloutPageGet` identifies a session and AgentInstance and accepts an
optional opaque forward cursor and limit. Hostd resolves the already-known
session directory, loads the durable shard through `SessionStorePort`,
filters by transcript sequence, and returns `RolloutPage { items,
next_cursor }`.

Cursors encode the last returned sequence as an implementation-private
`seq:<u64>` token. The default limit is 50 and the server clamps requests to
1–200. Hostd reads one extra row to determine whether to issue a next cursor.
Unknown sessions and malformed cursors fail explicitly; reads never create
storage.

## Ownership

The v3 AgentInstance JSONL remains the only durable rollout fact. Protocol
contains paging DTOs, hostd owns storage lookup and paging policy, and
clients consume pages without affecting session state.

## Verification

Protocol serde tests cover optional cursor/limit fields. Hostd integration
tests create a durable run and prove ordered, non-overlapping cursor pages.
