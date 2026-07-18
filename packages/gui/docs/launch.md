# Launching piko-gui

macOS-first desktop client for hostd Sessions. The GUI never talks to orchd and
never owns durable Session state — hostd remains authoritative.

## Build and run

From the workspace root:

```bash
cargo build -p piko-gui
cargo run -p piko-gui
```

The window title includes the current working directory’s leaf name. The left
Session sidebar lists Sessions for that cwd by default.

## hostd discovery

`piko-gui` resolves the hostd binary via [`dependency-pins.md`](dependency-pins.md)
discovery rules (`PIKO_HOSTD` / sibling target paths). Startup fails fast if the
binary cannot be spawned.

Useful environment variables:

| Variable | Purpose |
|---|---|
| `PIKO_HOSTD_PATH` / `PIKO_HOSTD_COMMAND` | Absolute path or command for `piko-hostd` |
| `PIKO_LOG_DISABLE` | Disable hostd file logging when set |
| `PIKO_LOG_FILE` / `PIKO_LOG_LEVEL` | Hostd logging (when enabled) |
| `PIKO_SESSION_DIR` | Override hostd Session storage root (primarily tests/packaging) |

## Keyboard (first release)

| Binding | Action |
|---|---|
| `cmd-n` | New Session |
| `cmd-b` / `cmd-i` | Toggle Sessions / Inspector (Sheet on narrow) |
| `cmd-l` | Focus Composer |
| `cmd-j` | Jump to latest Timeline |
| `cmd-.` | Cancel Turn |

## Validation smoke

With a built `piko-hostd` on the discovery path:

```bash
cargo test -p piko-gui hostd_smoke -- --nocapture
```

See also [`manual-ux-checklist.md`](manual-ux-checklist.md) and
[`release-checklist.md`](release-checklist.md).
