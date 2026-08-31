#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./dsh-tui-common.sh
source "$script_dir/dsh-tui-common.sh"

repo_root="$(dsh_tui_repo_root "$script_dir")"
tui_profile="${DSH_TUI_PROFILE:-$dsh_tui_default_profile}"
dsh_tui_validate_profile_name "$tui_profile"

usage() {
  printf '%s\n' \
    'Usage: scripts/start-new-chat.sh [--check] [--skip-setup]' \
    '' \
    'Starts the Grok TUI with a new DeepSeek Harness session.' \
    '  --check  verify the backend handshake without creating a session' \
    '' \
    'Default backend (pager flags; DSH_TUI_SERVER is not set):' \
    '  --backend <node|node.exe>' \
    '  --backend-arg <absolute apps/cli/lib/bin.js>' \
    '  --backend-arg --profile' \
    '  --backend-arg <profile>' \
    '' \
    'Optional overrides:' \
    '  DSH_HARNESS_ROOT  DeepSeek Harness checkout' \
    '  DSH_HOME          Harness home (default: $HOME/.dsh)' \
    '  DSH_TUI_PROFILE   DSH profile (default: dsh-pager-grok-controllers-v2-dev)' \
    '  DSH_TUI_PROFILE_ALLOW_UPDATE=1  allow updating a non-project profile' \
    '  DSH_TUI_SERVER    advanced override of the complete backend command;' \
    '                    split on whitespace, so paths must not contain spaces.' \
    '                    When set, the default --backend flags are not injected.' \
    '  DSH_TUI_CARGO     Cargo executable' \
    '' \
    'By default the local TypeScript packages are built and linked into the profile.' \
    'Use --skip-setup only when the profile has already been prepared.'
}

check_only=0
skip_setup=0
while (( $# > 0 )); do
  case "$1" in
    --check)
      check_only=1
      ;;
    --skip-setup)
      skip_setup=1
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
  shift
done

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

use_env_backend=0
if [[ -n "${DSH_TUI_SERVER:-}" ]]; then
  use_env_backend=1
else
  if (( skip_setup == 0 )); then
    "$repo_root/scripts/setup-dev-profile.sh"
  else
    dsh_tui_require_profile "$tui_profile" "$repo_root"
  fi
  dsh_tui_prepare_pager_backend "$repo_root" "$tui_profile"
fi

cd "$repo_root"
"$cargo_program" build -p dsh-pager-bin --locked

pager="$repo_root/target/debug/dsh-pager"
if (( check_only == 1 )); then
  if (( use_env_backend == 1 )); then
    printf 'Checking TUI backend: %s\n' "$DSH_TUI_SERVER" >&2
    exec "$pager" --hello
  fi
  printf 'Checking TUI backend: %s %s --profile %s\n' \
    "$dsh_tui_node_program" "$dsh_tui_harness_entry" "$tui_profile" >&2
  exec "$pager" --hello "${dsh_tui_pager_backend_argv[@]}"
fi

printf 'Starting a new conversation with profile %s. Press Ctrl+C to exit.\n' \
  "$tui_profile" >&2
# This project-specific launcher promises the colored Grok TUI surface. Some
# agent shells inject NO_COLOR for command output; do not let that unrelated
# parent preference collapse the running rail gradient into a static line.
# TERM and COLORTERM remain untouched so crossterm still sees the real terminal.
if (( use_env_backend == 1 )); then
  exec env -u NO_COLOR "$pager"
fi
exec env -u NO_COLOR "$pager" "${dsh_tui_pager_backend_argv[@]}"
