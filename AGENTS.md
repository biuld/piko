# AGENTS.md — piko project context

## Project overview

piko is a coding agent harness with a Host + stateless Engine architecture. It reimplements [pi](https://github.com/earendil-works/pi-mono) by splitting the monolithic runtime into two layers: a stateful Host (sessions, TUI, settings, auth, skills, prompts, compaction) and a stateless Engine (LLM calls, tool execution, approval state machine).

The guiding principle: **replicate pi's functionality, keep the host+engine split clean.**

## Architecture

```
cli → host-tui → host-runtime → engine-native → engine-protocol
                                    ↕
                              engine-remote (JSON-RPC)
```

- `engine-protocol/` — Pure types, zero deps. `EngineInput`, `EngineEvent`, `EngineStepResult`, `StatelessEngine`, `EventStream`.
- `engine-native/` — In-process engine: state machine, tool runner, built-in tools (read/bash/edit/write/grep/find/ls).
- `engine-remote/` — JSON-RPC client for remote engines.
- `host-runtime/` — Host core: `PikoHost`, `runScheduler`, `SessionManager`, `SettingsManager`, `ModelRegistry`, `AuthStorage`, compaction, skills, prompt templates, context files, resource loader.
- `host-tui/` — Terminal UI: `runTui`, `ChatView`, overlays (model/thinking/settings/login/resume/tree/fork), extensions, themes, tool rendering.
- `cli/` — `piko` binary: argument parsing, model resolution, TUI launch.

## Key files

| File | Purpose |
|---|---|
| `packages/host-runtime/src/scheduler.ts` | Agent loop: retry, prepareTurn, approval |
| `packages/host-runtime/src/host.ts` | PikoHost: system prompt, compaction, session ops |
| `packages/engine-native/src/state-machine.ts` | Engine step + approval resolution |
| `packages/engine-native/src/tool-runner.ts` | Tool execution with approval gating |
| `packages/engine-native/src/tools/registry.ts` | Built-in tool definitions |
| `packages/host-runtime/src/session/session-manager.ts` | Full session lifecycle |
| `packages/host-runtime/src/settings/manager.ts` | Layered settings (global → project → CLI) |
| `packages/host-runtime/src/models/registry.ts` | Model discovery + auth integration |
| `packages/host-runtime/src/auth/storage.ts` | API key / OAuth credential storage |
| `packages/host-runtime/src/prompts/system-prompt.ts` | System prompt builder (skills, context, tools, templates) |
| `packages/host-tui/src/tui-app.ts` | TUI main: layout, streaming, model cycling, extensions |
| `packages/host-tui/src/chat-view.ts` | Message rendering (user, assistant, tool, branch/compaction summary) |
| `packages/cli/src/cli.ts` | CLI entrypoint: wires SettingsManager, ModelRegistry, AuthStorage |

## Coding conventions

- **TypeScript strict mode** across all packages
- **Project references** (`tsconfig.json` `references`) for build ordering
- **ESM modules** with `.js` extension imports (Node.js ESM convention)
- **No circular dependencies** between packages
- **Tests** use `vitest`; run per-package with `npx vitest run`
- **Exports** in each package's `index.ts` are the public API

## When adding features

1. If it involves LLM interaction or tool execution → engine-native or engine-protocol
2. If it involves session, settings, auth, models, prompts, skills, compaction → host-runtime
3. If it involves UI, overlays, rendering, themes, extensions → host-tui
4. If it involves CLI arguments, print/json/rpc modes, piped stdin → cli
5. Types shared across Host and Engine → engine-protocol

## Session storage

Sessions are stored as JSONL under `~/.piko/sessions/<encoded-cwd>/<session-id>.jsonl`. The format is pi-compatible (from `packages/host-runtime/src/session/pi/types.ts`).

## Configuration

- `~/.piko/settings.json` — global settings (default model, theme, thinking level, compaction)
- `~/.piko/auth.json` — API keys per provider
- `.piko/settings.json` — project settings (overrides global)
- `.piko/skills/*.md` — project skills
- `.piko/prompts/*.md` — project prompt templates
- `.piko/themes/*.json` — project themes

## Before committing

Always run formatting and lint before committing:

```bash
npm run fmt    # biome check --fix
npm run check  # biome check && tsc -b
```

## Testing

```bash
# Per package
cd packages/engine-native && npx vitest run
cd packages/host-runtime && npx vitest run

# Engine tests use FauxProvider (mock LLM)
# Host tests use FauxProvider + in-memory sessions
```

## Pi reference

When implementing features from pi-mono, the reference files are at:
- `/Users/biu/Projects/pi-mono/packages/agent/src/agent-loop.ts`
- `/Users/biu/Projects/pi-mono/packages/agent/src/harness/agent-harness.ts`
- `/Users/biu/Projects/pi-mono/packages/coding-agent/src/`

Not all pi features are implemented — see `docs/implementation-gaps.md` for the current delta.
