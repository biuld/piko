# D-27: F-03 mention-syntax parsing

> Status: implemented
> Implements: [F-03](../features/F-03-prompt-assembly.md) mention-syntax slice

## Goal

When a user message contains `@path` and/or `$skill` mentions (plain text as
already emitted by TUI file placeholders), hostd resolves them against the
session workspace and skill catalog and injects retained, data-only Context
messages so the model sees file and skill bodies without a first-round tool
call.

## Constraints and non-goals

- User message text stays the original expanded submit string (`@path` / `$name`
  visible); durable User message is not rewritten.
- Paths must resolve under the session cwd (fail-soft notice if not).
- Skill `$name` requires a loaded skill of that name; unknown names get a
  fail-soft notice Context.
- No plugin mentions (`plugin://`); plugins deferred with F-14.
- No UI autocomplete changes (TUI `@` file browser already exists).
- No frozen `PromptBlock` catalog entries for mention bodies (RunDynamic
  Context on the transcript, like world-state / F-20).

## Proposed design

### Ownership

| Concern | Owner |
|---|---|
| Parse tokens from user text | hostd `domain/prompts/mentions` (pure) |
| Resolve file path under cwd; read body | hostd domain (fs) |
| Resolve skill body from catalog | hostd domain (skill `file_path` read) |
| Message shape + ids | `piko-protocol` helpers |
| Inject / commit chain | orchd `prepare_execution` (same prelude path as F-20) |
| Produce mention Context list | hostd `submit_chat` → `PromptResourceSnapshot.user_mentions` |

### Syntax

| Form | Example | Notes |
|---|---|---|
| File | `@src/main.rs` | Token-start `@` (not email: previous char not alnum/`_`); path runs until whitespace |
| Skill | `$review` | Token-start `$`; name `[A-Za-z0-9_./-]+`; env-var common false positives skipped (`$PATH`, `$HOME`, …) |

Linked forms `[$x](…)` and plugin `plugin://` are out of this slice.

### Content contracts

File success:

```text
file mention:
path: <display path under cwd, forward slashes>
---
<body, UTF-8, truncated>
```

File failure (outside cwd / missing / binary / IO):

```text
file mention:
path: <raw path>
error: <stable short reason>
```

Skill success:

```text
skill mention:
name: <skill name>
location: <skill file path>
---
<body>
```

Skill unknown:

```text
skill mention:
name: <name>
error: unknown skill
```

Body max: `FILE_MENTION_MAX_CHARS = 64_000` chars (truncate with trailing `…`).

### Protocol / wire

```rust
// PromptResourceSnapshot
pub user_mentions: Vec<Message>, // Context, order of first appearance

// StartExecutionRequest
pub user_mentions: Vec<Message>,
```

Chain (durable):

```text
head → world_state? → inter_agent_completions… → user_mentions… → user input
```

Message ids:

```rust
format!("{execution_id}/file-mention/{index}")
format!("{execution_id}/skill-mention/{index}")
```

Source kinds: `user.file-mention`, `user.skill-mention`; locator = display
path or skill name. Trust: `WorkspaceControlled` for file/skill bodies;
`Trusted` for pure error notices is optional — use `WorkspaceControlled`
uniformly for simplicity.

### Submit path

```text
expanded_text = expand_prompt_template(text)
tokens = parse_mentions(&expanded_text)
for each file token → resolve + build Context
for each skill token → skill lookup + load body → Context
prompt_resources.user_mentions = messages // appearance order, dedupe by locator
prompt = expanded_text (unchanged)
```

Dedup: first file path / skill name wins if mentioned twice in one message.

### orchd

`run_protocol` copies `prompt_resources.user_mentions` into
`StartExecutionRequest`. `prepare_execution` + `ExecutionActor` inject the
same order as F-20 completions.

## Verification

[V-27](../verification/V-27-mention-syntax.md)
