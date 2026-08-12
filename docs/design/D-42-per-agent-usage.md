# D-42: Per-agent usage projection and TUI

> Status: accepted
> Implements: [F-30](../features/F-30-per-agent-usage.md)
> Decisions: existing host-authority and durable-message ledger decisions in D-29

## Goal

Replace the low-value TUI `/status` modal with a host-authoritative `/usage`
surface that attributes durable usage and execution time to AgentInstances.

## Constraints and non-goals

- Durable assistant-message usage remains the token/cost fact; no second usage
  store is introduced.
- Durable `agent_executions` records remain the run timing fact.
- Protocol contains DTOs only.
- The TUI presents the projection and never re-aggregates transcript or
  execution records.
- No session-duration or cross-currency sum is invented.

## Proposed design

### Protocol projection

Add `AgentUsageSummary` and `SessionSnapshot.agent_usage`:

```rust
pub struct AgentUsageSummary {
    pub agent_instance_id: AgentInstanceId,
    pub agent_id: String,
    pub run_count: Option<u64>,
    pub active_duration_ms: Option<u64>,
    pub usage: Usage,
}
```

The snapshot vector is stable-sorted using the session agent tree order, with
instance id as the deterministic fallback. The new snapshot field defaults to
an empty vector for wire compatibility.

### Host aggregation

`SessionState` builds token/cost buckets by walking `SessionTreeEntry::Message`
entries. Each assistant message with usage accumulates into the bucket keyed by
the entry's `agent_instance_id`. Agent metadata supplies rows and display ids
even when usage is empty.

During `HostApp::enrich_session_view`, hostd opens the known session store and
reads its manifest. Every `AgentExecutionManifestEntry` increments `run_count`.
Completed records add `finished_at - started_at`; running records add
`snapshot_at - started_at`. Negative or malformed intervals contribute zero.
The enrichment merges timing into the usage buckets and sorts rows alongside
the authoritative agent list.

If runtime timing facts are unavailable, hostd leaves `run_count` and
`active_duration_ms` absent instead of publishing fabricated zeros. Durable
token/cost accounting remains available from the session transcript.

### TUI state and refresh

`AppState` stores `snapshot.agent_usage` and replaces it on reconciliation.
Opening `/usage` mounts `SurfaceId::Usage` and sends `StateSnapshot` for the
active session. The existing reconciliation path refreshes both the modal and
the rest of the client projection atomically.

The centered table renders:

- agent label and a marker for the viewed instance;
- run count;
- input, output, cache-read, and total tokens;
- active execution duration;
- formatted provider-native cost ledger.

A final session row uses `SessionSnapshot.cumulative_usage` and leaves run/time
blank. Narrow terminals use a stacked row form rather than silently dropping
the accounting scope.

`/status`, `SurfaceId::Status`, and the old queue/tool/notification renderer are
removed. The key action and configurable action id become `Usage` and
`app.usage`.

## Package impact

| Package | Change |
|---|---|
| `piko-protocol` | Add per-agent usage snapshot DTO and compatible field |
| `piko-hostd` | Aggregate durable message usage and execution timing into snapshots |
| `piko-tui` | Store, refresh, and render `/usage`; remove `/status` presentation |

## Reusable infrastructure

No `island-rs` change required.

## Failure and cancellation

- Cancelled and failed executions retain their finished interval and committed
  partial usage.
- Interrupted executions are terminalized by the existing recovery path before
  snapshot aggregation.
- A running record uses the snapshot timestamp only; no durable mutation occurs
  when reading usage.
- Snapshot failure leaves the existing client projection intact and follows the
  standard command-error feedback path.

## Verification

- Protocol serde default test for snapshots without `agentUsage`.
- Host domain tests for per-agent usage separation and accumulation.
- Host application/storage test for run count, completed duration, running
  duration, and resume.
- TUI snapshot test for replacing per-agent usage state.
- TUI command test that `/usage` opens the surface and requests a snapshot.
- TUI renderer/format tests for empty rows, token abbreviations, duration, and
  multi-entry costs.

## Alternatives considered

- **Keep `/status` and add usage fields:** rejected because the old modal mixes
  unrelated scopes and has no coherent user job.
- **TUI aggregates Timeline entries:** rejected because Timeline is a selected
  agent presentation and hostd owns user-visible durable state.
- **Session age as duration:** rejected because idle time is not agent work.
- **Model-step duration only:** deferred because existing durable facts describe
  complete agent runs and give a recoverable product measure.

## Rollout

1. Add compatible protocol DTOs and host aggregation.
2. Add snapshot state and `/usage` renderer.
3. Remove `/status` wiring and update TUI documentation.
4. Record acceptance evidence after targeted and workspace verification.
