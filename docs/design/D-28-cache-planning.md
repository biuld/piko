# D-28: F-03 cache-planning polish

> Status: implemented
> Implements: [F-03](../features/F-03-prompt-assembly.md) cache-planning slice

## Goal

Make the frozen-prompt **cache plan** accurately separate stable prefix tiers so
operator-visible changes invalidate only the right segment: project files no
longer share a cache identity with tool/skill catalogs, and operators can set
the provider cache policy without rebuilding hosts.

## Constraints and non-goals

- hostd remains authoritative for block labels and digests; llmd only maps
  the plan into provider options (cache key + message breakpoint).
- RunDynamic / NoCache blocks stay **outside** `prefix_segments` and must
  never change `semantic_prefix_digest`.
- Non-goals: multi-breakpoint hierarchical provider breakpoints beyond the
  existing single Ephemeral marker on the last stable chat message; moving
  `environment.host` into the stable prefix (stays RunDynamic).

## Proposed design

### 1. Scope reassignment (segment independence)

| Block id | Was | Now |
|---|---|---|
| `catalog.skills` | ResourceSnapshot | **CatalogStable** |
| `catalog.prompt-templates` | ResourceSnapshot | **CatalogStable** |
| `project.context.*` | ResourceSnapshot | ResourceSnapshot (unchanged) |
| tool catalog | segment `catalog_digest` only | unchanged (CatalogStable) |

Effect on digests:

- Changing AGENTS.md / project context → ResourceSnapshot segment only.
- Changing skills or templates or tool catalog → CatalogStable segment only.
- Platform / operator / agent unchanged.

### 2. Cache policy plumbing

- `PromptResourceSnapshot.cache_policy: PromptCachePolicy` (serde default
  `ProviderDefault`).
- `assemble_agent_run_prompt` copies it into `PromptCachePlan.policy`.
- Host settings:

```toml
[prompt]
cache-policy = "provider-default" # disabled | provider-default | ephemeral | extended
```

`HostSettings.prompt: Option<PromptSettings>` with kebab-case serde;
`submit_chat` / prompt materials path stamps the resolved policy onto the
snapshot.

### 3. Prefix composition (unchanged algorithm, documented)

`cache_segments` still emits non-empty tiers in order:

```text
GlobalStable → OperatorStable → AgentStable → CatalogStable → ResourceSnapshot
```

`CatalogStable` always carries `tool_catalog.digest` even when no catalog
blocks exist (tools-only invalidation).

`semantic_prefix_digest = hash(assembly_version + serialize(prefix_segments))`.

### 4. Assembly version

Bump `AGENT_RUN_PROMPT_ASSEMBLY_VERSION` **4 → 5** because catalog blocks
change cache scope (old digests must not pair with new segment labels).

### 5. llmd mapping (no behavior change required beyond policy)

Existing split remains the wire mapping:

- System message: high-authority Instruction blocks (Platform/Operator/Agent).
- Stable user message: remaining non-RunDynamic blocks (now correctly
  includes CatalogStable-only catalog text; not project when projects absent).
- Dynamic user message: RunDynamic / NoCache.
- Single cache breakpoint on the last pre-dynamic message when
  `policy != Disabled`.
- `provider_cache_key` uses `semantic_prefix_digest` as today.

## Tests

1. Project content change flips ResourceSnapshot / prefix; tool catalog fixed
   keeps CatalogStable digest.
2. Skill catalog change (or template) flips CatalogStable without project change.
3. Tool catalog change flips prefix independent of project.
4. RunDynamic still orthogonal to prefix.
5. `cache_policy = Disabled` propagates into `cache_plan.policy`.
6. Assembly version is `5`.

## Verification

[V-28](../verification/V-28-cache-planning.md)
