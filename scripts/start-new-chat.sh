#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tui_profile="${DSH_TUI_PROFILE:-grok-tui}"

usage() {
  printf '%s\n' \
    'Usage: scripts/start-new-chat.sh [--check]' \
    '' \
    'Starts the Grok TUI with a new DeepSeek Harness session.' \
    '  --check  verify the backend handshake without creating a session' \
    '' \
    'Optional overrides:' \
    '  DSH_HARNESS_ROOT  DeepSeek Harness checkout' \
    '  DSH_TUI_PROFILE   DSH profile (default: grok-tui)' \
    '  DSH_TUI_SERVER    complete backend command' \
    '  DSH_TUI_CARGO     Cargo executable'
}

check_only=0
case "${1:-}" in
  '') ;;
  --check)
    check_only=1
    shift
    ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
if (( $# > 0 )); then
  usage >&2
  exit 2
fi

if [[ -n "${DSH_TUI_CARGO:-}" ]]; then
  cargo_program="$DSH_TUI_CARGO"
elif command -v cargo >/dev/null 2>&1; then
  cargo_program="$(command -v cargo)"
elif [[ -x /home/leo/.cargo/bin/cargo ]]; then
  cargo_program=/home/leo/.cargo/bin/cargo
else
  printf 'Cargo was not found; set DSH_TUI_CARGO to its executable path.\n' >&2
  exit 2
fi
if [[ ! -x "$cargo_program" ]]; then
  printf 'Cargo is not executable: %s\n' "$cargo_program" >&2
  exit 2
fi

if [[ -n "${DSH_TUI_SERVER:-}" ]]; then
  server_program="$DSH_TUI_SERVER"
else
  if [[ -n "${DSH_HARNESS_ROOT:-}" ]]; then
    harness_root="$DSH_HARNESS_ROOT"
  elif [[ -x /home/leo/aidreamschool/deepseek-harness/apps/cli/lib/bin.js ]]; then
    harness_root=/home/leo/aidreamschool/deepseek-harness
  else
    harness_root=/home/leo/code/deepseek-harness
  fi
  harness_entry="$harness_root/apps/cli/lib/bin.js"
  if [[ ! -x "$harness_entry" ]]; then
    printf 'DeepSeek Harness entry is not executable: %s\n' "$harness_entry" >&2
    printf 'Set DSH_HARNESS_ROOT or DSH_TUI_SERVER to override it.\n' >&2
    exit 2
  fi
  server_program="$harness_entry --profile $tui_profile"
fi

cd "$repo_root"
"$cargo_program" build -p dsh-pager-bin --locked

export DSH_TUI_SERVER="$server_program"
pager="$repo_root/target/debug/dsh-pager"
if (( check_only == 1 )); then
  printf 'Checking TUI backend: %s\n' "$server_program" >&2
  exec "$pager" --hello
fi

printf 'Starting a new conversation with profile %s. Press Ctrl+C to exit.\n' \
  "$tui_profile" >&2
exec "$pager" --new
