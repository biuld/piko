# hostd logging

hostd owns global tracing initialization for itself and orchd. orchd only emits `tracing` events; it never installs a subscriber.

## Defaults

hostd always logs to a file by default:

| Mode | Output | Filter |
|------|--------|--------|
| `hostd` (no flags) | `~/.piko/logs/hostd-<timestamp>.log` | `info,hostd=info,orchd=info` |
| `hostd --log-stderr` | file + stderr | configured filter |
| `hostd --no-log` | stderr only | configured filter |
| TUI (no flags) | same default file via env | info (hostd default) |
| TUI `--debug` | default file | `debug,hostd=debug,orchd=debug` |
| TUI `--no-log` | none (stderr discarded) | — |

Log directories are created as `~/.piko/logs/` with mode `0700` when missing.

## hostd CLI / environment

```
hostd [--log-file PATH] [--log-level FILTER] [--log-stderr] [--no-log]
```

| Variable | Meaning |
|----------|---------|
| `PIKO_LOG_FILE` | Override log file path (`~` expanded) |
| `PIKO_LOG_LEVEL` | EnvFilter string (falls back to `RUST_LOG`) |
| `PIKO_LOG_STDERR=1` | Also mirror logs to stderr when a file is configured |
| `PIKO_LOG_DISABLE=1` | Disable file logging (stderr only) |

Priority: CLI flags > environment > defaults.

## TUI integration

The TUI passes logging overrides to hostd via environment variables (never inherits hostd stderr, so ratatui stays clean):

```
cargo run -p tui
cargo run -p tui -- --debug
cargo run -p tui -- --log-file ~/.piko/logs/repro.log --log-level info,orchd=debug
cargo run -p tui -- --no-log
```

When file logging is active, the TUI prints `Logging to <path>` before entering the alternate screen.

## Debugging turn lifecycle

```bash
cargo run -p tui -- --debug
tail -f ~/.piko/logs/hostd-*.log | rg "idle|persist|projection|stopping task|turn observation"
```

A healthy turn should show, in order: turn observation loop starting → assistant message persisted → ExecutionChanged (Running → Succeeded/Failed/Cancelled) → turn observation loop finished.
