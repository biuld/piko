# Launching piko-gui

macOS-first desktop client for hostd Sessions. The GUI never talks to orchd and
never owns durable Session state — hostd remains authoritative.

## Build and run

From the workspace root:

```bash
cargo build -p piko-gui
cargo run -p piko-gui
```

`cargo run` launches a bare binary: the Dock uses the generic executable glyph.
For a branded Dock / Finder icon, build the macOS app bundle:

```bash
./packages/gui/scripts/bundle-macos.sh
open target/Piko.app
```

The bundle ships `AppIcon.icns` and a sibling `piko-hostd` discovered via
`PIKO_HOSTD_PATH`. See [GUI App Identity & Safe Quit](features/app-identity-quit.md).

The TitleBar shows the brand mark only. Session and project context live in
the left Sessions island, which lists all Sessions globally, grouped by working
directory and sorted alphabetically.

## hostd discovery

`piko-gui` resolves the hostd binary via [`dependency-pins.md`](dependency-pins.md)
discovery rules (`PIKO_HOSTD_PATH` / `PIKO_HOSTD_COMMAND` / sibling target
paths). Startup fails fast if the binary cannot be spawned.

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
| `cmd-n` | New Session in the live Session's directory; Open Directory when idle |
| `cmd-b` / `cmd-i` | Toggle Sessions / Agents+Tree (TitleBar icons; Sheet when undockable) |
| `cmd-l` | Focus Composer |
| `tab` / `shift-tab` | Cycle focus among visible islands |
| `cmd-j` | Jump to latest Timeline |
| `cmd-.` | Cancel Turn |
| `cmd-q` | Quit (confirms when a turn is running or an approval is pending) |

## Validation smoke

With a built `piko-hostd` on the discovery path:

```bash
cargo test -p piko-gui hostd_smoke -- --nocapture
```

See also [`known-limitations.md`](known-limitations.md).
