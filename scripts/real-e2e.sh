#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
harness_root="${DSH_HARNESS_ROOT:-/home/leo/code/deepseek-harness}"
server_program="${DSH_TUI_SERVER:-$harness_root/apps/cli/lib/bin.js --profile tui-embedded}"
timeout_seconds="${REAL_E2E_TIMEOUT:-45}"
session_id="${REAL_E2E_SESSION:-}"

if [[ ! -x "$harness_root/apps/cli/lib/bin.js" ]]; then
  printf 'real Harness entry is not executable: %s\n' "$harness_root/apps/cli/lib/bin.js" >&2
  exit 2
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
  --backend-arg tui-embedded \
  --timeout "$timeout_seconds"

printf 'real DeepSeek Harness E2E checks passed (hello/list/dashboard/load/PTY)\n'
