#!/usr/bin/env bash
# Build piko-hostd + piko-tui, then run the TUI against that hostd.
#
# Why: `cargo run -p piko-tui` only rebuilds the UI client. Agent tools,
# orchd, and session authority live in piko-hostd. A stale hostd binary
# silently serves an old tool surface (e.g. missing list_agent_specs).
#
# Usage:
#   ./scripts/dev-tui.sh
#   ./scripts/dev-tui.sh -c
#   ./scripts/dev-tui.sh -m deepseek-v4-flash
#   PIKO_DEV_PROFILE=release ./scripts/dev-tui.sh
#
# Extra cargo build flags: PIKO_CARGO_BUILD_FLAGS="--features …"
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=dev-common.sh
source "$ROOT/scripts/dev-common.sh"

piko_dev_build_and_run tui "$@"
