# F-14: Skills, plugins, and hooks

> Status: partial (skills baseline implemented; plugins and hooks deferred)
> Priority: P1
> Source evidence: codex-rs `core/src/skills.rs`,
> `core/src/mcp_skill_dependencies.rs`, `core/src/plugins/*`,
> `core/src/hook_runtime.rs`; piko F-03 prompt assembly and mention syntax

## Summary

piko discovers workspace and user skills, exposes safe skill metadata to the
model, and lets the model load a matching skill's instructions or the user
explicitly inject a skill with `$name`. Invalid skills fail soft without
blocking a turn. Plugin discovery and hooks remain deferred until piko has a
concrete product journey that needs them.

## Problem

Reusable task instructions should not be copied into every prompt or hardcoded
into the runtime. Operators need a project-local and user-level skill catalog
whose relevant entries are discoverable by the model while full instruction
bodies are loaded only when needed. Malformed third-party content must not
make the host unavailable, and project definitions must be able to override
broader definitions predictably.

codex-rs also groups plugins and hooks into this ecosystem block. piko has no
current consumer for plugin installation, plugin-provided tools, or hook
execution, so defining those contracts before a journey exists would create
unused coupling.

## User journeys

1. An operator adds a skill under `.piko/skills/`. The next turn exposes the
   skill name, description, and location in the catalog available to the
   model.
2. The model recognizes that a cataloged skill matches the task, reads its
   file, resolves relative references from the skill directory, and follows
   the instructions.
3. A user writes `$review-rust` in a message. If that skill exists, the turn
   receives a retained Context message containing its bounded body before the
   user message; an unknown or unreadable skill produces a short error Context.
4. A project defines the same skill name as a broader user-level catalog. The
   definition nearest the session cwd wins.
5. One skill has malformed frontmatter. hostd warns and omits that skill while
   the remaining catalog and turn continue normally.

## In scope

### Skills baseline

- Discover skill definitions in `.piko/skills/` and `.agents/skills/` while
  walking from the session cwd toward the user home directory.
- Accept a directory whose root is `SKILL.md`, nested skill directories, and
  Markdown definitions directly under a skills directory.
- Resolve duplicate names by proximity: the definition found closest to the
  cwd wins.
- Require a non-empty description and validate skill names. Malformed,
  unreadable, or description-less definitions are omitted fail-soft; invalid
  names currently remain loaded with diagnostics for compatibility.
- Parse `name`, `description`, `disable-model-invocation`, `model`, `thinking`,
  and `tools` frontmatter. Only discovery visibility is currently affected:
  `disable-model-invocation = true` removes the skill from the model-visible
  catalog. Parsed model/thinking/tool values are reserved metadata and do not
  alter runtime selection in this slice.
- Expose only bounded catalog metadata in the frozen prompt. Full skill bodies
  are loaded on demand through the normal read tool or through F-03 `$skill`
  mention expansion.
- Treat the skill catalog as `CatalogStable` prompt-cache material.
- Preserve F-03 explicit `$skill` mention behavior and bounded retained
  Context messages.

## Out of scope

- Plugin discovery, installation, enablement, rendering, or plugin-provided
  tools and MCP dependencies.
- Plugin mention syntax.
- Hook execution for additional context or input inspection.
- Applying skill `model`, `thinking`, or `tools` metadata as runtime overrides.
- Downloading skills or mutating the skill catalog during a turn.
- Treating skill content as trusted system policy; it remains
  workspace-controlled instruction material.

## Behavior and states

### Loading

- Missing skill directories produce an empty catalog without an error.
- Discovery walks outward from cwd; the first valid definition for a name is
  retained and broader duplicates are ignored.
- A directory-level `SKILL.md` represents that skill root. Hidden entries and
  `node_modules` are skipped.

### Valid skill

- The model-visible catalog contains the skill's name, description, and file
  location.
- The prompt tells the model to read a matching skill file and to resolve its
  relative references against the skill directory.
- A visible skill participates in the catalog cache digest.

### Hidden, malformed, or unreadable skill

- `disable-model-invocation = true` omits the skill from the catalog without
  deleting it from the loaded host representation.
- Missing descriptions and malformed/unreadable files are omitted with a
  diagnostic; other skills remain available.
- Invalid names produce diagnostics but remain loaded in this baseline. Name
  validation does not grant a skill extra trust or filesystem access.

### Explicit mention

F-03 owns parsing and durable transcript placement. `$name` resolves against
the same loaded catalog and injects a bounded, data-only Context message. An
unknown or unreadable definition fails soft with an error Context.

## Acceptance criteria

- [x] A project skill with valid `name` and `description` appears in the
      model-visible catalog with its location.
- [x] The loader accepts nested `SKILL.md` and direct Markdown definitions and
      prefers a nearer project definition over a broader definition with the
      same name.
- [x] Malformed frontmatter and missing required descriptions omit only the
      affected skill and emit a diagnostic.
- [x] YAML-like arrays and booleans used by `tools` and
      `disable-model-invocation` parse into normalized metadata.
- [x] A skill with `disable-model-invocation = true` is absent from the
      model-visible catalog.
- [x] The prompt explains on-demand file loading and skill-relative path
      resolution without inlining full skill bodies.
- [x] Skill catalog changes invalidate the CatalogStable cache segment without
      invalidating the ResourceSnapshot segment.
- [x] F-03 `$skill` mention expansion uses the catalog and injects a bounded
      retained Context or a fail-soft error Context.

## Product decisions

| Question | Decision | Rationale |
|---|---|---|
| Where are skills owned? | hostd prompt materials | hostd owns cwd resources and the user-visible prompt snapshot |
| What is prompt-visible by default? | metadata only | keeps the stable prefix bounded; bodies load only when relevant |
| Duplicate resolution | nearest definition wins | project intent overrides broader user defaults |
| Failure policy | warn, omit one skill, continue | optional instruction material must not block the agent runtime |
| Skill frontmatter overrides | parse but do not apply | runtime model/tool policy needs a separate product contract and safety review |
| Plugins/hooks | deferred until a consumer | avoids designing unused installation and execution authority |

## Fusion decisions (codex-rs)

| codex-rs behavior | Decision | piko landing / rationale |
|---|---|---|
| Skill discovery and prompt metadata | **kept (adapted)** | hostd loads cwd/user catalogs into F-03 typed prompt fragments |
| On-demand skill body loading | **kept (adapted)** | model uses the read tool; `$skill` is an explicit retained Context path |
| Skill model/tool overrides | **rejected (deferred)** | metadata is parsed, but no piko journey authorizes it to change runtime policy |
| Plugin discovery/install/list | **rejected (deferred)** | no current piko consumer or authority model |
| Plugin mentions and MCP dependencies | **rejected (deferred)** | land only with a plugin PRD and concrete journey |
| Additional-context/input-inspection hooks | **rejected (deferred)** | no hook consumer; hostd prompt and admission paths remain explicit |

## Open questions

1. Which product journey, if any, should allow a skill to select a model,
   thinking level, or tool subset?
2. If plugins are scheduled, what installation trust and update model should
   hostd enforce before loading plugin-provided content?

## Reference evidence

- `packages/hostd/src/adapters/prompts/skill_loader.rs`
- `packages/hostd/src/domain/prompts/skills.rs`
- `packages/hostd/src/domain/prompts/build.rs`
- `packages/hostd/src/domain/prompts/mentions.rs`
- `packages/hostd/tests/resources.rs`
- [F-03 Prompt Assembly](F-03-prompt-assembly.md)
- [D-14 Skills Baseline](../design/D-14-skills-baseline.md)
- [V-14 Skills Baseline](../verification/V-14-skills-baseline.md)
