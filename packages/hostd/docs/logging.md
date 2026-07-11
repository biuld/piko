# hostd logging

hostd owns global tracing initialization for itself and orchd. orchd only emits `tracing` events; it never installs a subscriber.

## Defaults

| Mode | Output | Filter |
|------|--------|--------|
| Standalone `hostd` | stderr | `info,hostd=info,orchd=info` |
| TUI spawn (no flags) | none | — |
| TUI `--debug` or `PIKO_LOG_FILE` | file under `~/.piko/logs/` | debug or configured level |

Log directories are created as `~/.piko/logs/` with mode `0700` when missing.

## hostd CLI / environment

```
hostd [--log-file PATH] [--log-level FILTER] [--log-stderr]
```

| Variable | Meaning |
|----------|---------|
| `PIKO_LOG_FILE` | Log file path (`~` expanded) |
| `PIKO_LOG_LEVEL` | EnvFilter string (falls back to `RUST_LOG`) |
| `PIKO_LOG_STDERR=1` | Also mirror logs to stderr when a file is configured |

Priority: CLI flags > environment > defaults.

## TUI integration

The TUI passes logging configuration to hostd via environment variables (never inherits hostd stderr, so ratatui stays clean):

```
cargo run -p tui -- --debug
cargo run -p tui -- --log-file ~/.piko/logs/repro.log --log-level info,orchd=debug
PIKO_LOG_FILE=~/.piko/logs/test.log cargo run -p tui
```

When file logging is enabled, the TUI prints `Logging to <path>` before entering the alternate screen.

## Debugging turn lifecycle

With `--debug`, tail the log while reproducing a stuck spinner:

```bash
tail -f ~/.piko/logs/hostd-*.log | rg "idle|persist|projection|stopping task|turn observation"
```

A healthy turn should show, in order: turn observation loop starting → assistant message persisted → step finished without tools / task idle lifecycle → root task terminal → turn observation loop finished.
