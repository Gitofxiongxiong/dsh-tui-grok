#!/usr/bin/env bash

# Shared environment and profile helpers for the local DeepSeek Harness launchers.
# This file is sourced by scripts; it is not a standalone entry point.

dsh_tui_repo_root() {
  local script_dir="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
  cd "$script_dir/.." && pwd
}

dsh_tui_default_profile="dsh-pager-grok-dev"

dsh_tui_resolve_harness_root() {
  local repo_root="$1"
  local candidate

  if [[ -n "${DSH_HARNESS_ROOT:-}" ]]; then
    printf '%s\n' "$DSH_HARNESS_ROOT"
    return 0
  fi

  for candidate in \
    "$repo_root/../deepseek-harness" \
    "/home/leo/code/deepseek-harness" \
    "/home/leo/aidreamschool/deepseek-harness"; do
    if [[ -x "$candidate/apps/cli/lib/bin.js" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  printf 'DeepSeek Harness checkout was not found. Set DSH_HARNESS_ROOT.\n' >&2
  return 2
}

dsh_tui_require_harness_entry() {
  local harness_root="$1"
  local harness_entry="$harness_root/apps/cli/lib/bin.js"
  if [[ ! -x "$harness_entry" ]]; then
    printf 'DeepSeek Harness entry is not executable: %s\n' "$harness_entry" >&2
    printf 'Set DSH_HARNESS_ROOT or DSH_TUI_SERVER to override it.\n' >&2
    return 2
  fi
  printf '%s\n' "$harness_entry"
}

dsh_tui_resolve_node() {
  if command -v node >/dev/null 2>&1; then
    command -v node
    return 0
  fi
  printf 'Node.js was not found; install Node.js or set PATH before running the TUI.\n' >&2
  return 2
}

dsh_tui_pnpm() {
  if command -v pnpm >/dev/null 2>&1; then
    pnpm "$@"
  elif command -v corepack >/dev/null 2>&1; then
    corepack pnpm "$@"
  else
    printf 'pnpm was not found; install pnpm (or enable Corepack) before setup.\n' >&2
    return 2
  fi
}

dsh_tui_dsh_home() {
  printf '%s\n' "${DSH_HOME:-${HOME:?}/.dsh}"
}

dsh_tui_profile_dir() {
  local profile="$1"
  printf '%s/profiles/%s\n' "$(dsh_tui_dsh_home)" "$profile"
}

dsh_tui_validate_profile_name() {
  local profile="$1"
  case "$profile" in
    ''|.|..|node_modules|*/*|*\\*)
      printf 'Invalid DSH profile name: %s\n' "$profile" >&2
      return 2
      ;;
  esac
}

dsh_tui_profile_manifest_is_managed() {
  local manifest="$1"
  local node_program
  node_program="$(dsh_tui_resolve_node)"
  [[ -f "$manifest" ]] || return 1

  "$node_program" - "$manifest" <<'NODE'
const fs = require('node:fs')

const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const bundles = manifest?.dsh?.profile?.bundles ?? []
const dependencies = Object.keys(manifest?.dependencies ?? {})
const managed = bundles.includes('@dsh-pager-grok/tui-embedded')
  || dependencies.some((name) => name.startsWith('@dsh-pager-grok/tui-'))
process.exit(managed ? 0 : 1)
NODE
}

dsh_tui_profile_manifest_is_ready() {
  local manifest="$1"
  local node_program
  node_program="$(dsh_tui_resolve_node)"
  [[ -f "$manifest" ]] || return 1

  "$node_program" - "$manifest" <<'NODE'
const fs = require('node:fs')

const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const bundles = manifest?.dsh?.profile?.bundles ?? []
const dependencies = manifest?.dependencies ?? {}
const expectedDependencies = [
  '@dsh-pager-grok/tui-protocol',
  '@dsh-pager-grok/tui-server',
  '@dsh-pager-grok/tui-embedded',
  '@dsh-pager-grok/tui-session-projection-recovery',
]
const ready = bundles.includes('@dsh-pager-grok/tui-embedded')
  && expectedDependencies.every((name) => Object.hasOwn(dependencies, name))
process.exit(ready ? 0 : 1)
NODE
}

dsh_tui_profile_manifest_links_current_repo() {
  local manifest="$1"
  local repo_root="$2"
  local node_program
  node_program="$(dsh_tui_resolve_node)"
  [[ -f "$manifest" ]] || return 1

  "$node_program" - "$manifest" "$repo_root" <<'NODE'
const fs = require('node:fs')

const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const repoRoot = process.argv[3]
const dependencies = manifest?.dependencies ?? {}
const expected = {
  '@dsh-pager-grok/tui-protocol': `link:${repoRoot}/packages/dsh-tui-protocol`,
  '@dsh-pager-grok/tui-server': `link:${repoRoot}/packages/dsh-tui-server`,
  '@dsh-pager-grok/tui-embedded': `link:${repoRoot}/packages/dsh-tui-embedded`,
  '@dsh-pager-grok/tui-session-projection-recovery': `link:${repoRoot}/packages/dsh-tui-session-projection-recovery`,
}
const current = Object.entries(expected).every(([name, spec]) => dependencies[name] === spec)
process.exit(current ? 0 : 1)
NODE
}

dsh_tui_require_profile() {
  local profile="$1"
  local repo_root="$2"
  local profile_dir
  local manifest
  profile_dir="$(dsh_tui_profile_dir "$profile")"
  manifest="$profile_dir/package.json"

  if [[ ! -f "$manifest" ]]; then
    printf 'DSH profile %s is not initialized: %s\n' "$profile" "$profile_dir" >&2
    printf 'Run %s/scripts/setup-dev-profile.sh first.\n' "$repo_root" >&2
    return 2
  fi
  if ! dsh_tui_profile_manifest_is_ready "$manifest"; then
    printf 'DSH profile %s is missing one or more dsh-pager-grok packages/bundles.\n' \
      "$profile" >&2
    printf 'Run %s/scripts/setup-dev-profile.sh or choose another DSH_TUI_PROFILE.\n' \
      "$repo_root" >&2
    return 2
  fi
}

dsh_tui_ensure_workspace_dependencies() {
  local repo_root="$1"
  if [[ ! -d "$repo_root/node_modules" ]]; then
    printf 'Installing JavaScript workspace dependencies...\n' >&2
    (cd "$repo_root" && dsh_tui_pnpm install --frozen-lockfile)
  fi
}

dsh_tui_ensure_typescript_build() {
  local repo_root="$1"
  printf 'Building local dsh-pager-grok TypeScript packages...\n' >&2
  (cd "$repo_root" && dsh_tui_pnpm run build:ts)
}

dsh_tui_install_local_profile() {
  local repo_root="$1"
  local harness_entry="$2"
  local profile="$3"
  local profile_dir
  local manifest
  profile_dir="$(dsh_tui_profile_dir "$profile")"
  manifest="$profile_dir/package.json"

  if dsh_tui_profile_manifest_is_ready "$manifest" \
    && dsh_tui_profile_manifest_links_current_repo "$manifest" "$repo_root"; then
    printf 'DSH profile %s is already linked to this checkout.\n' "$profile" >&2
    return 0
  fi

  if [[ -f "$manifest" ]] && ! dsh_tui_profile_manifest_is_managed "$manifest" \
    && [[ "${DSH_TUI_PROFILE_ALLOW_UPDATE:-0}" != "1" ]]; then
    printf 'DSH profile %s already exists and is not owned by dsh-pager-grok: %s\n' \
      "$profile" "$profile_dir" >&2
    printf 'Choose a project profile with DSH_TUI_PROFILE, or set DSH_TUI_PROFILE_ALLOW_UPDATE=1.\n' >&2
    return 2
  fi

  printf 'Preparing DSH profile %s under %s...\n' "$profile" "$(dsh_tui_dsh_home)" >&2
  "$harness_entry" plugin --profile "$profile" add \
    "$repo_root/packages/dsh-tui-protocol" \
    "$repo_root/packages/dsh-tui-server" \
    "$repo_root/packages/dsh-tui-embedded" \
    "$repo_root/packages/dsh-tui-session-projection-recovery"

  dsh_tui_require_profile "$profile" "$repo_root"
  printf 'DSH profile %s is ready.\n' "$profile" >&2
}
