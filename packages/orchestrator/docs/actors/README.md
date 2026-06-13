# Actors

Core piko business actors built on top of the generic actor kernel.

## Core Actors

| Actor | ID | Owns |
| --- | --- | --- |
| [MainActor](main-actor.md) | `orchestrator:main` | top-level run/task coordination |
| [AgentActor](agent-actor.md) | `agent:<agentId>` | agent transcript, engine loop, task state |
| [ToolActor](tool-actor.md) | `tool:registry` or tool namespace | tool discovery, policy, provider execution |
| [StateActor](state-actor.md) | `orchestrator:state` | event log, reducer projection, subscriptions |

Each actor owns its private runtime state. Cross-actor communication goes
through `send()` / `ask()` / `reply()` only.

Approval/user interaction is handled by `HostToolProvider`. File write
serialization, if needed, belongs inside concrete write-capable tools/providers,
not ToolActor.
