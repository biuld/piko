#!/usr/bin/env bash
# Build piko-hostd + piko-gui, then run the GUI against that hostd.
#
# Same hostd-freshness guarantee as scripts/dev-tui.sh.
#
# Usage:
#   ./scripts/dev-gui.sh
#   ./scripts/dev-gui.sh --help
#   PIKO_DEV_PROFILE=release ./scripts/dev-gui.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=dev-common.sh
source "$ROOT/scripts/dev-common.sh"

piko_dev_build_and_run gui "$@"
