#!/usr/bin/env bash
set -euo pipefail

fixture_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${DSH_PAGER_GROK_ROOT:-$(cd "$fixture_dir/../../.." && pwd)}"
export DSH_PAGER_GROK_ROOT="$repo_root"
compat_version="${DSH_COMPAT_VERSION:-0.1.1-rc.2}"
case "$compat_version" in
  0.1.1-rc.2) version_label="rc2" ;;
  0.1.0-rc.8) version_label="rc8" ;;
  *)
    printf 'Unsupported fixture version: %s\n' "$compat_version" >&2
    exit 2
    ;;
esac
export DSH_COMPAT_VERSION="$compat_version"
profile="dsh-pager-grok-apiproxy-v1-${version_label}-e2e"
timeout_seconds="${REAL_E2E_TIMEOUT:-45}"
node_program="$(command -v node)"
dsh_entry="$fixture_dir/node_modules/@deepseek-ai/dsh/lib/bin.js"
export PATH="$fixture_dir/bin:$PATH"

if [[ ! -r "$dsh_entry" ]]; then
  printf 'rc.2 fixture is not installed: %s\n' "$dsh_entry" >&2
  printf 'Run pnpm --dir %s install --frozen-lockfile first.\n' "$fixture_dir" >&2
  exit 2
fi

e2e_parent="${TMPDIR:-/tmp}"
e2e_home="$(mktemp -d "$e2e_parent/dsh-pager-phase4b-${version_label}.XXXXXX")"
cleanup() {
  if [[ "${KEEP_DSH_HOME:-0}" == "1" ]]; then
    printf 'Preserved %s DSH_HOME: %s\n' "$compat_version" "$e2e_home" >&2
  elif [[ -n "${e2e_home:-}" && -d "$e2e_home" \
    && "$e2e_home" == "$e2e_parent"/dsh-pager-phase4b-"$version_label".* ]]; then
    rm -rf -- "$e2e_home"
  fi
}
trap cleanup EXIT INT TERM
export DSH_HOME="$e2e_home"

printf '%s fixture DSH_HOME: %s\n' "$compat_version" "$DSH_HOME"
printf '%s profile: %s\n' "$compat_version" "$profile"
printf '%s CLI: %s %s\n' "$compat_version" "$node_program" "$dsh_entry"

corepack pnpm@11.20.0 --pm-on-fail=ignore --dir "$repo_root" run build:ts
corepack pnpm@11.7.0 --pm-on-fail=ignore --dir "$fixture_dir" run build
"$node_program" "$dsh_entry" plugin --profile "$profile" list
cp "$fixture_dir/profile-pnpm-workspace.yaml" "$DSH_HOME/profiles/$profile/pnpm-workspace.yaml"
"$node_program" "$dsh_entry" plugin --profile "$profile" add "$fixture_dir"

cargo build --manifest-path "$repo_root/Cargo.toml" -p dsh-pager-bin --locked
binary="$repo_root/target/debug/dsh-pager"
backend_args=(
  --backend "$node_program"
  --backend-arg "$dsh_entry"
  --backend-arg --profile
  --backend-arg "$profile"
)

run_pager() {
  timeout "${timeout_seconds}s" "$binary" "$@" "${backend_args[@]}"
}

run_pager --hello
run_pager --list-sessions
run_pager --dashboard
run_pager --load-only --new

DSH_HOME="$DSH_HOME" python3 "$repo_root/scripts/pty-smoke.py" \
  --binary "$binary" \
  --pager-arg=--new \
  --backend "$node_program" \
  "--backend-arg=$dsh_entry" \
  "--backend-arg=--profile" \
  "--backend-arg=$profile" \
  --timeout "$timeout_seconds"

printf '%s real E2E passed (hello/list/dashboard/load/PTY)\n' "$compat_version"
