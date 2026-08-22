#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT
TEST_HOME="$TEST_ROOT/user-home"
INSTALL_ROOT="$TEST_HOME/.piko"
mkdir -p "$TEST_HOME"

if [[ -n "${PIKO_TEST_TUI_BINARY:-}" && -n "${PIKO_TEST_HOSTD_BINARY:-}" ]]; then
  tui_binary="$PIKO_TEST_TUI_BINARY"
  hostd_binary="$PIKO_TEST_HOSTD_BINARY"
else
  mkdir -p "$TEST_ROOT/fake"
  printf '#!/bin/sh\nexit 0\n' >"$TEST_ROOT/fake/piko-tui"
  printf '#!/bin/sh\nexit 0\n' >"$TEST_ROOT/fake/piko-hostd"
  chmod +x "$TEST_ROOT/fake/piko-tui" "$TEST_ROOT/fake/piko-hostd"
  tui_binary="$TEST_ROOT/fake/piko-tui"
  hostd_binary="$TEST_ROOT/fake/piko-hostd"
fi

HOME="$TEST_HOME" \
SHELL="/bin/zsh" \
PIKO_HOME="$INSTALL_ROOT" \
PIKO_TUI_BINARY="$tui_binary" \
PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build >/dev/null

test -x "$INSTALL_ROOT/bin/piko"
test -x "$INSTALL_ROOT/bin/piko-hostd"
test -f "$INSTALL_ROOT/settings.toml"
test -f "$INSTALL_ROOT/keybindings.json"
test -f "$INSTALL_ROOT/agents/spec/main.toml"
test -d "$INSTALL_ROOT/agents/sessions"
test -f "$INSTALL_ROOT/models/openai.toml"
test -f "$INSTALL_ROOT/themes/dark.toml"
test -d "$INSTALL_ROOT/prompts"
test -d "$INSTALL_ROOT/skills"
test -f "$INSTALL_ROOT/env"
grep -Fqx '# piko shell setup' "$TEST_HOME/.zshrc"
grep -Fq "$INSTALL_ROOT/env" "$TEST_HOME/.zshrc"

if [[ "$(uname -s)" != "Windows_NT" ]]; then
  test "$(stat -f '%Lp' "$INSTALL_ROOT/auth.json" 2>/dev/null || stat -c '%a' "$INSTALL_ROOT/auth.json")" = "600"
fi

printf 'user-owned\n' >"$INSTALL_ROOT/settings.toml"
HOME="$TEST_HOME" \
SHELL="/bin/zsh" \
PIKO_HOME="$INSTALL_ROOT" \
PIKO_TUI_BINARY="$tui_binary" \
PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build >/dev/null
test "$(sed -n '1p' "$INSTALL_ROOT/settings.toml")" = "user-owned"
test "$(grep -Fc '# piko shell setup' "$TEST_HOME/.zshrc")" = "1"

BASH_HOME="$TEST_ROOT/bash-home"
mkdir -p "$BASH_HOME"
HOME="$BASH_HOME" SHELL="/bin/bash" PIKO_HOME="$BASH_HOME/.piko" \
PIKO_TUI_BINARY="$tui_binary" PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build >/dev/null
grep -Fqx '# piko shell setup' "$BASH_HOME/.bashrc"
grep -Fqx '# piko shell setup' "$BASH_HOME/.bash_profile"

FISH_HOME="$TEST_ROOT/fish-home"
mkdir -p "$FISH_HOME"
HOME="$FISH_HOME" SHELL="/opt/homebrew/bin/fish" PIKO_HOME="$FISH_HOME/.piko" \
PIKO_TUI_BINARY="$tui_binary" PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build >/dev/null
grep -Fq 'fish_add_path' "$FISH_HOME/.config/fish/conf.d/piko.fish"

OPT_OUT_HOME="$TEST_ROOT/opt-out-home"
mkdir -p "$OPT_OUT_HOME"
HOME="$OPT_OUT_HOME" SHELL="/bin/zsh" PIKO_HOME="$OPT_OUT_HOME/.piko" \
PIKO_TUI_BINARY="$tui_binary" PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build --no-modify-path >/dev/null
test ! -e "$OPT_OUT_HOME/.zshrc"

MIGRATION_HOME="$TEST_ROOT/migration-home"
MIGRATION_ROOT="$MIGRATION_HOME/.piko"
mkdir -p "$MIGRATION_ROOT/agents" "$MIGRATION_ROOT/agent/sessions/cwd_project"
printf 'custom agent\n' >"$MIGRATION_ROOT/agents/custom.toml"
printf 'durable session\n' >"$MIGRATION_ROOT/agent/sessions/cwd_project/marker"
HOME="$MIGRATION_HOME" SHELL="/bin/zsh" PIKO_HOME="$MIGRATION_ROOT" \
PIKO_TUI_BINARY="$tui_binary" PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build --no-modify-path >/dev/null
test "$(cat "$MIGRATION_ROOT/agents/spec/custom.toml")" = "custom agent"
test "$(cat "$MIGRATION_ROOT/agents/sessions/cwd_project/marker")" = "durable session"
test ! -e "$MIGRATION_ROOT/agents/custom.toml"
test ! -e "$MIGRATION_ROOT/agent"

CONFLICT_HOME="$TEST_ROOT/conflict-home"
CONFLICT_ROOT="$CONFLICT_HOME/.piko"
mkdir -p "$CONFLICT_ROOT/agents/spec"
printf 'legacy version\n' >"$CONFLICT_ROOT/agents/main.toml"
printf 'new version\n' >"$CONFLICT_ROOT/agents/spec/main.toml"
if HOME="$CONFLICT_HOME" SHELL="/bin/zsh" PIKO_HOME="$CONFLICT_ROOT" \
  PIKO_TUI_BINARY="$tui_binary" PIKO_HOSTD_BINARY="$hostd_binary" \
  "$ROOT/scripts/install.sh" --no-build --no-modify-path >/dev/null 2>&1; then
  echo "expected conflicting agent spec migration to fail" >&2
  exit 1
fi
test "$(cat "$CONFLICT_ROOT/agents/main.toml")" = "legacy version"
test "$(cat "$CONFLICT_ROOT/agents/spec/main.toml")" = "new version"

echo "installer test passed"
