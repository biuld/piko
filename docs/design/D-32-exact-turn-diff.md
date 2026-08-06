# D-32: Exact turn-diff tracking

> Status: accepted
> Implements: [F-15](../features/F-15-observability.md)

## Goal

Report the exact net text changes made by one turn without a racy post-turn
workspace scan.

## Design

Successful built-in `edit` and `write` handlers attach a private
`_pikoFileChange` record containing path and exact optional before/after text
to ToolResult details. Orchd persists those details but strips the private
key from the model-visible result.

After durable commit, hostd projects each record into the source turn. It
merges by path using first-before/latest-after, sorts paths, removes net-zero
changes, and emits `TurnDiff`. Unified text is deterministic and
content-exact; it intentionally uses one coarse hunk per file rather than a
minimal line-diff algorithm.

`TurnDiffGet` first reads hostd live state. If absent, it scans all durable
AgentInstance shards for matching `source_turn_id` ToolResults and performs
the same merge. Reconstruction never reads the workspace, so later edits do
not alter historical output.

## Boundaries

Only declared built-in text mutations participate. Process commands and MCP
tools cannot promise exact before/after state and do not invent records.
Failed built-in calls contribute nothing.

## Verification

Tests cover exact metadata, model-output sanitization, repeated-write rollup,
net-zero removal, durable reconstruction, and protocol event/query handling.
