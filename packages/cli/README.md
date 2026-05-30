# piko-cli

CLI product shell for piko. Wires together the engine, host, and upstream components.

## Usage

```bash
# Single prompt (non-interactive)
piko -p "Explain quantum computing in one sentence"

# With specific model
piko -p "Hello" -m claude-sonnet-4-5-20250929

# List available models
piko --list-models

# Interactive mode
piko
```

## Interactive Commands

- `/help` — Show commands
- `/model <id>` — Switch model
- `/provider <name>` — Switch provider
- `/system <text>` — Set system prompt
- `/exit`, `/quit` — Exit

## Environment

Set API keys via environment variables (same as pi):

- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- etc.
