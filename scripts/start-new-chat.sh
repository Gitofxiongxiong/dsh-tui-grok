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
    'Optional overrides:' \
    '  DSH_HARNESS_ROOT  DeepSeek Harness checkout' \
    '  DSH_HOME          Harness home (default: $HOME/.dsh)' \
    '  DSH_TUI_PROFILE   DSH profile (default: dsh-pager-grok-dev)' \
    '  DSH_TUI_PROFILE_ALLOW_UPDATE=1  allow updating a non-project profile' \
    '  DSH_TUI_SERVER    complete backend command' \
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

if [[ -n "${DSH_TUI_SERVER:-}" ]]; then
  server_program="$DSH_TUI_SERVER"
else
  harness_root="$(dsh_tui_resolve_harness_root "$repo_root")"
  harness_entry="$(dsh_tui_require_harness_entry "$harness_root")"
  if (( skip_setup == 0 )); then
    "$repo_root/scripts/setup-dev-profile.sh"
  else
    dsh_tui_require_profile "$tui_profile" "$repo_root"
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
