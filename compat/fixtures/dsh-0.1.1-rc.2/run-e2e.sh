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
owns_e2e_home=1
if [[ -n "${DSH_COMPAT_E2E_HOME:-}" ]]; then
  e2e_home="$DSH_COMPAT_E2E_HOME"
  mkdir -p "$e2e_home"
  owns_e2e_home=0
else
  e2e_home="$(mktemp -d "$e2e_parent/dsh-pager-phase4b-${version_label}.XXXXXX")"
fi
cleanup() {
  if (( owns_e2e_home == 0 )); then
    printf 'Caller-owned %s DSH_HOME retained: %s\n' "$compat_version" "$e2e_home" >&2
  elif [[ "${KEEP_DSH_HOME:-0}" == "1" ]]; then
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

if [[ "${DSH_COMPAT_SKIP_REPO_BUILD:-0}" != "1" ]]; then
  corepack pnpm@11.20.0 --pm-on-fail=ignore --dir "$repo_root" run build:ts
else
  test -r "$repo_root/packages/dsh-pager-runtime-apiproxy-v1/lib/server/core/serve.js"
  test -r "$repo_root/packages/dsh-pager-runtime-apiproxy-v1/lib/server/adapters/apiproxy-v1/backend.js"
fi
corepack pnpm@11.7.0 --pm-on-fail=ignore --dir "$fixture_dir" run build
"$node_program" "$dsh_entry" plugin --profile "$profile" list
cp "$fixture_dir/profile-pnpm-workspace.yaml" "$DSH_HOME/profiles/$profile/pnpm-workspace.yaml"
"$node_program" "$dsh_entry" plugin --profile "$profile" add "$fixture_dir"
"$node_program" --input-type=module - \
  "$repo_root/packages/dsh-pager-cli/lib/launcher.js" \
  "$repo_root/packages/dsh-pager-cli/package.json" \
  "$repo_root/compat/dsh-support.json" \
  "$profile" \
  "$compat_version" <<'NODE'
import fs from 'node:fs'
import { pathToFileURL } from 'node:url'

const [, , launcherPath, cliManifestPath, registryPath, profile, version] = process.argv
const { writeProfileOwnership } = await import(pathToFileURL(launcherPath))
const cli = JSON.parse(fs.readFileSync(cliManifestPath, 'utf8'))
const registry = JSON.parse(fs.readFileSync(registryPath, 'utf8'))
const support = registry?.versions?.[version]
if (support === undefined) throw new Error(`DSH ${version} is absent from the support registry`)
writeProfileOwnership(cli.version, {
  family: support.family,
  version,
  profileSchema: support.profileSchema,
  profile,
}, process.env)
NODE

if [[ "${DSH_COMPAT_SKIP_REPO_BUILD:-0}" != "1" ]]; then
  cargo build --manifest-path "$repo_root/Cargo.toml" -p dsh-pager-bin --locked
fi
binary="$repo_root/target/debug/dsh-pager"
test -x "$binary"
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
