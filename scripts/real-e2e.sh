#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=./dsh-tui-common.sh
source "$(dirname "$0")/dsh-tui-common.sh"

tui_profile="${DSH_TUI_PROFILE:-$dsh_tui_default_profile}"
dsh_tui_validate_profile_name "$tui_profile"
timeout_seconds="${REAL_E2E_TIMEOUT:-45}"
session_id="${REAL_E2E_SESSION:-}"
install_local="${DSH_TUI_INSTALL_LOCAL:-0}"
harness_root="$(dsh_tui_resolve_harness_root "$repo_root")"
harness_entry="$(dsh_tui_require_harness_entry "$harness_root")"

if [[ "$install_local" == "1" && -n "${DSH_TUI_SERVER:-}" ]]; then
  printf 'DSH_TUI_INSTALL_LOCAL=1 cannot be combined with a custom DSH_TUI_SERVER\n' >&2
  exit 2
fi

if [[ -n "${DSH_TUI_SERVER:-}" ]]; then
  server_program="$DSH_TUI_SERVER"
else
  server_program="$harness_entry --profile $tui_profile"

  if [[ "$install_local" == "1" ]]; then
    "$repo_root/scripts/setup-dev-profile.sh"
  else
    dsh_tui_require_profile "$tui_profile" "$repo_root"
  fi
fi

cd "$repo_root"
cargo build -p dsh-pager-bin --locked

export DSH_TUI_SERVER="$server_program"
binary="$repo_root/target/debug/dsh-pager"

run_pager() {
  timeout "${timeout_seconds}s" "$binary" "$@"
}

printf 'real Harness root: %s\n' "$harness_root"
printf 'real Harness command: %s\n' "$server_program"
run_pager --hello
run_pager --list-sessions
run_pager --dashboard

pager_session_args=()
if [[ -n "$session_id" ]]; then
  pager_session_args=(--session "$session_id")
  printf 'using existing session: %s\n' "$session_id"
else
  pager_session_args=(--new)
  printf 'using an isolated new session (set REAL_E2E_SESSION for read-only attach)\n'
fi
run_pager --load-only "${pager_session_args[@]}"

python3 scripts/pty-smoke.py \
  --binary "$binary" \
  --pager-arg=--new \
  --backend "$harness_root/apps/cli/lib/bin.js" \
  --backend-arg=--profile \
  --backend-arg "$tui_profile" \
  --timeout "$timeout_seconds"

printf 'real DeepSeek Harness E2E checks passed (hello/list/dashboard/load/PTY)\n'
