#!/usr/bin/env bash
# Shared helpers for scripts/dev-tui.sh and scripts/dev-gui.sh.
# Not meant to be invoked directly.

piko_dev_build_and_run() {
  local client="$1"
  shift

  local profile="${PIKO_DEV_PROFILE:-debug}"
  local -a cargo_profile_flags=()
  local target_dir="target/debug"

  case "$profile" in
    debug) ;;
    release)
      cargo_profile_flags=(--release)
      target_dir="target/release"
      ;;
    *)
      echo "error: PIKO_DEV_PROFILE must be 'debug' or 'release' (got: $profile)" >&2
      return 1
      ;;
  esac

  local client_pkg client_bin
  case "$client" in
    tui)
      client_pkg=piko-tui
      client_bin=piko-tui
      ;;
    gui)
      client_pkg=piko-gui
      client_bin=piko-gui
      ;;
    *)
      echo "error: unknown client '$client' (expected tui or gui)" >&2
      return 1
      ;;
  esac

  # Optional extra flags for `cargo build` (word-split intentionally).
  # shellcheck disable=SC2206
  local -a extra_build_flags=(${PIKO_CARGO_BUILD_FLAGS:-})

  echo "==> cargo build -p piko-hostd -p ${client_pkg} (${profile})"
  # Under `set -u`, empty arrays are "unbound" on macOS bash 3.2 — expand only
  # when non-empty (same pattern for optional extra build flags).
  cargo build -p piko-hostd -p "$client_pkg" \
    ${cargo_profile_flags[@]+"${cargo_profile_flags[@]}"} \
    ${extra_build_flags[@]+"${extra_build_flags[@]}"}

  local hostd_bin="${ROOT}/${target_dir}/piko-hostd"
  local client_path="${ROOT}/${target_dir}/${client_bin}"

  if [[ ! -x "$hostd_bin" ]]; then
    echo "error: hostd binary missing or not executable: $hostd_bin" >&2
    return 1
  fi
  if [[ ! -x "$client_path" ]]; then
    echo "error: client binary missing or not executable: $client_path" >&2
    return 1
  fi

  echo "==> hostd: $hostd_bin"
  if stat --version >/dev/null 2>&1; then
    # GNU stat
    echo "    mtime: $(stat -c '%y' "$hostd_bin")"
  else
    # BSD stat (macOS)
    echo "    mtime: $(stat -f '%Sm' -t '%Y-%m-%d %H:%M:%S' "$hostd_bin")"
  fi
  echo "==> client: $client_path"
  echo "==> running (PIKO_HOSTD_PATH=$hostd_bin)"

  # Force the just-built hostd even if PATH has another piko-hostd.
  export PIKO_HOSTD_PATH="$hostd_bin"
  exec "$client_path" "$@"
}
