# piko-cli

CLI product shell for piko. Wires together host-runtime and host-tui.

## Usage

```bash
# Interactive TUI mode
bun run piko
# or: bun packages/cli/bin/piko

# Continue most recent session
bun run piko -c

# Specify model
bun run piko -m claude-sonnet-4-5-20250929

# Set thinking level
bun run piko --thinking high

# List available models
bun run piko --list-models
```

## CLI Flags

| Flag | Description |
|---|---|
| `-m, --model <id>` | Model ID |
| `--provider <name>` | Provider name |
| `-c, --continue` | Continue most recent session |
| `--session <id>` | Resume a specific session |
| `--thinking <level>` | off \| minimal \| low \| medium \| high \| xhigh |
| `--api-key <key>` | API key for the provider |
| `--system-prompt <text>` | Custom system prompt |
| `--append-system-prompt <text>` | Append to default system prompt |
| `--name <name>` | Set session name |
| `--no-context-files` | Skip AGENTS.md / CLAUDE.md loading |
| `--no-tools` | Disable tool calling |
| `--session-dir <path>` | Custom session storage directory |
| `--prompt-template <name>` | Invoke a prompt template on startup |
| `--skill <name>` | Invoke a skill on startup |
| `--list-models` | List available models |
| `-h, --help` | Show help |

## Environment

Set API keys via environment variables:

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- etc.
