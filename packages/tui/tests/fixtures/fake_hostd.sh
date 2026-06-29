#!/bin/sh
created=0
while IFS= read -r command; do
  if [ -n "$PIKO_FAKE_HOSTD_LOG" ]; then
    printf '%s\n' "$command" >> "$PIKO_FAKE_HOSTD_LOG"
  fi
  printf '%s\n' '{"type":"command_accepted","command_id":"fake-command"}'
  case "$command" in
    *'"type":"session_create"'*)
      if [ "$created" -eq 0 ]; then
        created=1
        printf '%s\n' '{"type":"session_created","session_id":"fake-session","cwd":"/tmp/piko-smoke","timestamp":1}'
      fi
      ;;
  esac
done
