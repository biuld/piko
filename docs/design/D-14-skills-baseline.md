# D-14: Skills discovery and prompt exposure baseline

> Status: implemented
> Implements: [F-14](../features/F-14-skills-plugins.md) skills baseline

## Goal

Document the existing piko-native skills path as a complete vertical slice:
filesystem discovery, validation, precedence, typed prompt metadata, cache
planning, and explicit mention integration. Plugins, hooks, and runtime
model/tool overrides remain outside this design.

## Constraints and non-goals

- hostd remains authoritative for cwd-scoped prompt materials.
- Skill content is workspace-controlled, not system-trusted.
- orchd receives a frozen prompt snapshot and does not scan the filesystem.
- Full skill bodies do not enter the stable prompt catalog.
- No plugin loader, hook runtime, installer, or remote download path.
- Parsed `model`, `thinking`, and `tools` fields do not alter execution.

## Proposed design

### Ownership and flow

```text
session cwd
  -> hostd PromptMaterials::load_skills
  -> filesystem skill loader
  -> LoadSkillsResult { skills, diagnostics }
  -> prompt snapshot options
  -> catalog.skills PromptBlock (metadata only, CatalogStable)
  -> frozen AgentRunPrompt

submitted user text
  -> F-03 mention parser
  -> $name lookup in the same loaded catalog
  -> bounded retained Context before User message
```

hostd logs diagnostics and continues. orchd sees only the assembled prompt and
the retained mention Context messages carried by the execution request.

### Discovery and precedence

`skill_loader` walks from cwd toward the resolved user home. At every level it
checks `.piko/skills/` and `.agents/skills/`. Results are inserted by name
without replacement, so the first definition found closest to cwd wins.

A skills directory supports:

- a root `SKILL.md`, which defines that directory as one skill;
- nested directories containing `SKILL.md`;
- direct Markdown files at the skills-directory root.

Hidden entries and `node_modules` are skipped. Missing directories are normal
empty state.

### Parsing and validation

The shared prompt-frontmatter parser normalizes scalar, array, and boolean
values. The loader builds a `Skill` with:

- `name`, with the containing directory as fallback;
- required non-empty `description`;
- source `file_path` and `base_dir`;
- `disable_model_invocation`;
- reserved `model_override`, `thinking_level`, and `active_tools` metadata.

Unreadable files, malformed frontmatter, and missing descriptions return a
diagnostic and no skill. Name validation emits diagnostics for length or
character violations. The slice does not elevate parsed metadata into runtime
authority.

### Prompt assembly and cache planning

`domain::prompts::build` filters skills with
`disable_model_invocation = true`, sorts visible skills by name, and emits the
`catalog.skills` block with name, description, and location. The block source
is the workspace catalog and its cache scope is `CatalogStable`.

The catalog tells the model to use the read tool for the full file and resolve
relative paths against the skill directory. This keeps the stable prefix small
and makes body loading observable through the existing tool system.

F-03/D-28 folds the skills block into the catalog digest independently from
project resource snapshots.

### Explicit mention integration

F-03/D-27 parses `$name` after prompt-template expansion. Resolution uses the
same `Vec<Skill>` loaded for the run. The body is read at submission time and
bounded by the protocol mention limit. Success and fail-soft errors become
data-only retained Context messages; user text remains unchanged.

### Failure and cancellation

Skill discovery is synchronous bounded filesystem work during prompt snapshot
construction. A failure is isolated to one definition and reported as a
diagnostic. Missing or invalid skills never cancel a turn. Turn cancellation
continues through the normal hostd/orchd path after submission.

## Package impact

| Package | Responsibility |
|---|---|
| `piko-hostd` adapters | filesystem discovery and frontmatter parsing |
| `piko-hostd` domain | skill types, validation, catalog formatting, mention resolution |
| `piko-hostd` application | load once per submission and log diagnostics |
| `piko-protocol` | typed prompt block/cache scope and bounded mention Context |
| `piko-orchd` | consume the frozen prompt and durable Context chain; no discovery |

## Reusable infrastructure

- Existing prompt frontmatter parser.
- `PromptMaterials` port for cwd-scoped resources.
- F-03 typed prompt fragments and F-28 cache plan.
- F-03/D-27 retained mention messages.

## Verification

[V-14](../verification/V-14-skills-baseline.md) maps the F-14 acceptance
criteria to hostd resource integration tests and F-03 mention/cache evidence.

## Alternatives considered

- **Inline every skill body in the system prompt:** rejected because catalog
  growth would consume context and destabilize prompt caching.
- **Let orchd discover skills:** rejected because filesystem resource
  ownership and user-visible prompt state belong to hostd.
- **Apply model/tool overrides immediately:** rejected because it changes
  execution authority without a dedicated product and safety contract.
- **Build plugins and hooks with the baseline:** deferred because no current
  piko journey consumes them.

## Rollout

This design records behavior already present in the F-03 prompt pipeline. No
storage or protocol migration is required. Future F-14 slices must update the
Feature PRD before adding plugin, hook, or runtime-override behavior.
