# Slash Command Surface

Status: Implemented

## Purpose

The composer `/` trigger is the TUI's single command-discovery and invocation
surface. There is no separate command palette or command-palette shortcut.

The catalog merges TUI-local presentation actions with host-advertised product
commands. Slash aliases remain TUI-local; hostd remains authoritative for
product capabilities and invocation metadata.

## Behavior contract

1. Typing `/` at the start of the composer opens inline suggestions.
2. Search matches slash alias, title, and detail.
3. Immediate commands execute when accepted. Commands requiring arguments or
   confirmation insert the slash token and remain in the composer.
4. Missing required arguments show usage and preserve the submitted command so
   the user can continue editing it.
5. Destructive commands require explicit text confirmation; `/delete` alone
   never deletes a session.
6. Result lists use structured surfaces rather than notification text. MCP and
   process results use ComposerBand; diff and prompt debug share the reusable
   Diagnostics Browse surface.
7. Local commands are available before the host catalog bootstrap completes.
8. Host command failures are visible as errors.

## Command inventory

- Presentation: sessions, tree, model configuration, settings, status, agents,
  turn diff, prompt debug, quit.
- Host-owned: new, fork, rename, import, delete, login, logout, compact,
  process management, and MCP status when advertised by hostd.
- The TUI exposes one session-copy command, `/fork`, and one authentication
  entry command, `/login`. Lower-level host capabilities such as full-session
  clone, device-code login, and login cancellation are not separate slash
  commands.
- `/top` is the single process command. It opens the process ComposerBand;
  selecting a running process and pressing Enter arms an inline stop
  confirmation, and a second Enter invokes `process.stop`.
- `session.export` is not advertised until hostd provides an executable wire
  command and authoritative result.
