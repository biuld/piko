#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PIKO_INSTALL_ROOT="${PIKO_HOME:-${HOME:?HOME must be set}/.piko}"
PROFILE="release"
SKIP_BUILD=0
MODIFY_PATH=1

usage() {
  cat <<'USAGE'
Usage: scripts/install.sh [--debug] [--no-build] [--no-modify-path]

Build and install piko into $PIKO_HOME (default: ~/.piko).

Options:
  --debug       Install target/debug binaries instead of release binaries
  --no-build    Do not run cargo build (use already-built binaries)
  --no-modify-path
                Do not update the current shell's startup configuration
  -h, --help    Show this help

Environment:
  PIKO_HOME             Installation root
  PIKO_TUI_BINARY       TUI binary to install (primarily for packaging/tests)
  PIKO_HOSTD_BINARY     hostd binary to install (primarily for packaging/tests)
USAGE
}

while (($#)); do
  case "$1" in
    --debug) PROFILE="debug" ;;
    --no-build) SKIP_BUILD=1 ;;
    --no-modify-path) MODIFY_PATH=0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "error: unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ -z "$PIKO_INSTALL_ROOT" || "$PIKO_INSTALL_ROOT" == "/" ]]; then
  echo "error: refusing unsafe PIKO_HOME: '$PIKO_INSTALL_ROOT'" >&2
  exit 2
fi

if ((SKIP_BUILD == 0)); then
  cargo_args=(build -p piko-hostd -p piko-tui)
  if [[ "$PROFILE" == "release" ]]; then
    cargo_args+=(--release)
  fi
  echo "==> Building piko ($PROFILE)"
  (cd "$ROOT" && cargo "${cargo_args[@]}")
fi

target_dir="$ROOT/target/$PROFILE"
tui_binary="${PIKO_TUI_BINARY:-$target_dir/piko-tui}"
hostd_binary="${PIKO_HOSTD_BINARY:-$target_dir/piko-hostd}"
for binary in "$tui_binary" "$hostd_binary"; do
  if [[ ! -x "$binary" ]]; then
    echo "error: executable not found: $binary" >&2
    exit 1
  fi
done

migrate_agent_home() {
  local agents_root="$PIKO_INSTALL_ROOT/agents"
  local spec_root="$agents_root/spec"
  local legacy_agent_root="$PIKO_INSTALL_ROOT/agent"
  local source destination name

  mkdir -p "$spec_root"
  shopt -s nullglob

  for source in "$agents_root"/*.toml; do
    destination="$spec_root/$(basename "$source")"
    if [[ ! -e "$destination" ]]; then
      mv "$source" "$destination"
    elif cmp -s "$source" "$destination"; then
      rm "$source"
    else
      echo "error: agent spec migration conflict: $source -> $destination" >&2
      return 1
    fi
  done

  if [[ -d "$legacy_agent_root" ]]; then
    for source in \
      "$legacy_agent_root"/* \
      "$legacy_agent_root"/.[!.]* \
      "$legacy_agent_root"/..?*; do
      name="$(basename "$source")"
      destination="$agents_root/$name"
      if [[ -e "$destination" ]]; then
        echo "error: agent state migration conflict: $source -> $destination" >&2
        return 1
      fi
      mv "$source" "$destination"
    done
    rmdir "$legacy_agent_root"
  fi

  shopt -u nullglob
}

migrate_agent_home

mkdir -p \
  "$PIKO_INSTALL_ROOT/bin" \
  "$PIKO_INSTALL_ROOT/agents/spec" \
  "$PIKO_INSTALL_ROOT/agents/sessions" \
  "$PIKO_INSTALL_ROOT/models" \
  "$PIKO_INSTALL_ROOT/themes" \
  "$PIKO_INSTALL_ROOT/prompts" \
  "$PIKO_INSTALL_ROOT/skills"

install -m 0755 "$tui_binary" "$PIKO_INSTALL_ROOT/bin/piko"
install -m 0755 "$hostd_binary" "$PIKO_INSTALL_ROOT/bin/piko-hostd"

install_config() {
  local source="$1"
  local destination="$2"
  if [[ ! -e "$destination" ]]; then
    install -m 0644 "$source" "$destination"
    echo "    created ${destination#$PIKO_INSTALL_ROOT/}"
  fi
}

install_config "$ROOT/packages/hostd/resources/settings.toml" \
  "$PIKO_INSTALL_ROOT/settings.toml"
install_config "$ROOT/packages/tui/resources/keybindings.json" \
  "$PIKO_INSTALL_ROOT/keybindings.json"

for source in "$ROOT"/packages/hostd/resources/agents/*.toml; do
  install_config "$source" "$PIKO_INSTALL_ROOT/agents/spec/$(basename "$source")"
done
for source in "$ROOT"/packages/llmd/resources/models/*.toml; do
  install_config "$source" "$PIKO_INSTALL_ROOT/models/$(basename "$source")"
done
for source in "$ROOT"/packages/tui/resources/themes/*.toml; do
  install_config "$source" "$PIKO_INSTALL_ROOT/themes/$(basename "$source")"
done

if [[ ! -e "$PIKO_INSTALL_ROOT/auth.json" ]]; then
  printf '{}\n' >"$PIKO_INSTALL_ROOT/auth.json"
  chmod 0600 "$PIKO_INSTALL_ROOT/auth.json"
  echo "    created auth.json"
fi

shell_quote() {
  local value="$1"
  printf "'%s'" "${value//\'/\'\\\'\'}"
}

env_file="$PIKO_INSTALL_ROOT/env"
if [[ ! -e "$env_file" ]]; then
  quoted_bin="$(shell_quote "$PIKO_INSTALL_ROOT/bin")"
  printf '# piko shell setup\nexport PATH=%s:"$PATH"\n' "$quoted_bin" >"$env_file"
  chmod 0644 "$env_file"
  echo "    created env"
fi

shell_configured=0
if ((MODIFY_PATH == 1)); then
  shell_name="$(basename "${SHELL:-}")"
  marker="# piko shell setup"
  case "$shell_name" in
    zsh)
      rc_file="${ZDOTDIR:-$HOME}/.zshrc"
      mkdir -p "$(dirname "$rc_file")"
      touch "$rc_file"
      if ! grep -Fqx "$marker" "$rc_file"; then
        quoted_env="$(shell_quote "$env_file")"
        printf '\n%s\n[ -f %s ] && . %s\n' "$marker" "$quoted_env" "$quoted_env" >>"$rc_file"
      fi
      shell_configured=1
      ;;
    bash)
      quoted_env="$(shell_quote "$env_file")"
      for bash_rc in "$HOME/.bashrc" "$HOME/.bash_profile"; do
        touch "$bash_rc"
        if ! grep -Fqx "$marker" "$bash_rc"; then
          printf '\n%s\n[ -f %s ] && . %s\n' "$marker" "$quoted_env" "$quoted_env" >>"$bash_rc"
        fi
      done
      rc_file="$HOME/.bashrc, $HOME/.bash_profile"
      shell_configured=1
      ;;
    fish)
      rc_file="$HOME/.config/fish/conf.d/piko.fish"
      mkdir -p "$(dirname "$rc_file")"
      touch "$rc_file"
      if ! grep -Fqx "$marker" "$rc_file"; then
        quoted_bin="$(shell_quote "$PIKO_INSTALL_ROOT/bin")"
        printf '\n%s\nfish_add_path %s\n' "$marker" "$quoted_bin" >>"$rc_file"
      fi
      shell_configured=1
      ;;
  esac
fi

echo "==> Installed piko in $PIKO_INSTALL_ROOT"
if ((shell_configured == 1)); then
  echo "    Shell integration: $rc_file"
  if [[ "$shell_name" == "fish" ]]; then
    echo "    Restart fish, or run: fish_add_path $PIKO_INSTALL_ROOT/bin"
  else
    echo "    Restart your shell, or run: . $env_file"
  fi
else
  echo "    Add this directory to PATH (or source $env_file):"
  echo "    $PIKO_INSTALL_ROOT/bin"
fi
