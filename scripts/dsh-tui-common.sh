#!/usr/bin/env bash

# Shared environment and profile helpers for the local DeepSeek Harness launchers.
# This file is sourced by scripts; it is not a standalone entry point.
#
# Default pager backend is node + absolute apps/cli/lib/bin.js + --profile
# (step 1 product argv). Launchers pass --backend/--backend-arg; they do not
# export DSH_TUI_SERVER on the default path. bin.js must exist and be readable;
# it does not need to be executable.

dsh_tui_repo_root() {
  local script_dir="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"
  cd "$script_dir/.." && pwd
}

dsh_tui_default_profile="dsh-pager-grok-controllers-v2-dev"

# The source-only development profile is controllers-v2-specific. Resolve its
# exact DSH version from the canonical support registry instead of maintaining
# a shell version constant.
dsh_tui_required_harness_version() {
  local repo_root="${1:-$(dsh_tui_repo_root)}"
  local registry="$repo_root/compat/dsh-support.json"
  local node_program
  node_program="$(dsh_tui_resolve_node)" || return
  "$node_program" - "$registry" <<'NODE'
const fs = require('node:fs')
const registry = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const matches = Object.entries(registry?.versions ?? {})
  .filter(([, entry]) => entry.family === 'controllers-v2' && entry.distribution === 'source-only')
if (matches.length !== 1) {
  console.error(`Expected exactly one controllers-v2/source-only registry entry, found ${matches.length}`)
  process.exit(2)
}
process.stdout.write(`${matches[0][0]}\n`)
NODE
}

# Filled by dsh_tui_prepare_pager_backend for launchers.
dsh_tui_node_program=""
dsh_tui_harness_root=""
dsh_tui_harness_entry=""
dsh_tui_pager_backend_argv=()

dsh_tui_resolve_harness_root() {
  local repo_root="$1"
  local candidate
  local required_version
  required_version="$(dsh_tui_required_harness_version "$repo_root")" || return

  if [[ -n "${DSH_HARNESS_ROOT:-}" ]]; then
    dsh_tui_require_harness_checkout "$DSH_HARNESS_ROOT"
    return
  fi

  for candidate in \
    "$repo_root/../deepseek-harness-latest" \
    "$repo_root/../deepseek-harness" \
    "/home/leo/code/deepseek-harness" \
    "/home/leo/aidreamschool/deepseek-harness"; do
    [[ -d "$candidate" ]] || continue
    dsh_tui_require_harness_checkout "$candidate"
    return
  done

  printf 'DeepSeek Harness %s checkout was not found. Set DSH_HARNESS_ROOT.\n' \
    "$required_version" >&2
  return 2
}

dsh_tui_harness_version() {
  local harness_root="$1"
  local manifest="$harness_root/apps/cli/package.json"
  local node_program
  node_program="$(dsh_tui_resolve_node)" || return
  if [[ ! -r "$manifest" ]]; then
    printf 'DeepSeek Harness CLI manifest is missing or unreadable: %s\n' "$manifest" >&2
    return 2
  fi
  "$node_program" - "$manifest" <<'NODE'
const fs = require('node:fs')
const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
if (typeof manifest.version !== 'string') process.exit(2)
process.stdout.write(`${manifest.version}\n`)
NODE
}

dsh_tui_require_harness_checkout() {
  local harness_root="$1"
  local actual_version
  local required_version
  local repo_root
  repo_root="$(dsh_tui_repo_root)"
  required_version="$(dsh_tui_required_harness_version "$repo_root")" || return
  actual_version="$(dsh_tui_harness_version "$harness_root")" || return
  if [[ "$actual_version" != "$required_version" ]]; then
    printf 'Unsupported DeepSeek Harness version at %s: expected %s, found %s.\n' \
      "$harness_root" "$required_version" "$actual_version" >&2
    return 2
  fi
  dsh_tui_require_harness_entry "$harness_root" >/dev/null || return
  printf '%s\n' "$harness_root"
}

dsh_tui_require_harness_entry() {
  local harness_root="$1"
  local harness_entry="$harness_root/apps/cli/lib/bin.js"
  # Node prefixes the entry; the shebang bit is not required (Windows/Git Bash).
  if [[ ! -f "$harness_entry" || ! -r "$harness_entry" ]]; then
    printf 'DeepSeek Harness entry is missing or not readable: %s\n' "$harness_entry" >&2
    printf 'Set DSH_HARNESS_ROOT, or set DSH_TUI_SERVER as a no-whitespace override.\n' >&2
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

# Fill globals used by launchers:
#   dsh_tui_node_program
#   dsh_tui_harness_root
#   dsh_tui_harness_entry
#   dsh_tui_pager_backend_argv  (pager --backend / --backend-arg flags)
dsh_tui_prepare_pager_backend() {
  local repo_root="$1"
  local profile="${2:-${DSH_TUI_PROFILE:-$dsh_tui_default_profile}}"

  dsh_tui_node_program="$(dsh_tui_resolve_node)" || return
  dsh_tui_harness_root="$(dsh_tui_resolve_harness_root "$repo_root")" || return
  dsh_tui_harness_entry="$(dsh_tui_require_harness_entry "$dsh_tui_harness_root")" || return
  dsh_tui_pager_backend_argv=(
    --backend "$dsh_tui_node_program"
    --backend-arg "$dsh_tui_harness_entry"
    --backend-arg --profile
    --backend-arg "$profile"
  )
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
const managed = manifest?.dshPagerGrok?.managed === true
  || bundles.includes('@dsh-pager-grok/tui-embedded')
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
const ownership = manifest?.dshPagerGrok
const expectedDependencies = [
  '@dsh-pager-grok/tui-protocol',
  '@dsh-pager-grok/tui-server',
  '@dsh-pager-grok/tui-embedded',
  '@deepseek-ai/dsh-agent-presets',
  '@deepseek-ai/dsh-api-session-controller',
  '@deepseek-ai/dsh-api-settings-controller',
  '@deepseek-ai/dsh-api-workspace-controller',
  '@deepseek-ai/dsh-tool-subagent',
]
const ready = bundles.includes('@dsh-pager-grok/tui-embedded')
  && expectedDependencies.every((name) => Object.hasOwn(dependencies, name))
  && ownership?.managed === true
  && ownership?.adapterFamily === 'controllers-v2'
  && ownership?.profileSchema === 2
process.exit(ready ? 0 : 1)
NODE
}

dsh_tui_prepare_family_profile() {
  local repo_root="$1"
  local harness_root="$2"
  local profile="$3"
  local mode="${4:-prepare}"
  local node_program
  node_program="$(dsh_tui_resolve_node)" || return

  "$node_program" --input-type=module - \
    "$repo_root/packages/dsh-pager-cli/lib/launcher.js" \
    "$repo_root/packages/dsh-pager-cli/package.json" \
    "$repo_root/compat/dsh-support.json" \
    "$harness_root/apps/cli/package.json" \
    "$profile" \
    "$mode" <<'NODE'
import fs from 'node:fs'
import { pathToFileURL } from 'node:url'

const [, , launcherPath, cliManifestPath, registryPath, dshManifestPath, profile, mode] = process.argv
const { prepareFamilyProfile, writeProfileOwnership } = await import(pathToFileURL(launcherPath))
const cli = JSON.parse(fs.readFileSync(cliManifestPath, 'utf8'))
const registry = JSON.parse(fs.readFileSync(registryPath, 'utf8'))
const dsh = JSON.parse(fs.readFileSync(dshManifestPath, 'utf8'))
const support = registry?.versions?.[dsh.version]
if (support === undefined) throw new Error(`DSH ${dsh.version} is absent from the support registry`)
if (!profile.includes(`-${support.family}`)) {
  throw new Error(`profile name must include family ${support.family}: ${profile}`)
}
const selection = {
  family: support.family,
  version: dsh.version,
  profileSchema: support.profileSchema,
  profile,
}
if (mode === 'prepare') prepareFamilyProfile(cli.version, selection, process.env)
else if (mode === 'stamp') writeProfileOwnership(cli.version, selection, process.env)
else throw new Error(`unknown family profile mode: ${mode}`)
NODE
}

dsh_tui_profile_manifest_links_current_repo() {
  local manifest="$1"
  local repo_root="$2"
  local harness_root="$3"
  local node_program
  node_program="$(dsh_tui_resolve_node)"
  [[ -f "$manifest" ]] || return 1

  "$node_program" - "$manifest" "$repo_root" "$harness_root" <<'NODE'
const fs = require('node:fs')

const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const repoRoot = process.argv[3]
const harnessRoot = process.argv[4]
const dependencies = manifest?.dependencies ?? {}
const expected = {
  '@dsh-pager-grok/tui-protocol': `link:${repoRoot}/packages/dsh-tui-protocol`,
  '@dsh-pager-grok/tui-server': `link:${repoRoot}/packages/dsh-tui-server`,
  '@dsh-pager-grok/tui-embedded': `link:${repoRoot}/packages/dsh-tui-embedded`,
  '@deepseek-ai/dsh-api-session-controller': `link:${harnessRoot}/packages/api/session-controller`,
  '@deepseek-ai/dsh-api-settings-controller': `link:${harnessRoot}/packages/api/settings-controller`,
  '@deepseek-ai/dsh-api-workspace-controller': `link:${harnessRoot}/packages/api/workspace-controller`,
  '@deepseek-ai/dsh-tool-subagent': `link:${harnessRoot}/packages/subagent/tool-subagent`,
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
  local harness_root="$2"
  local harness_entry="$3"
  local profile="$4"
  local profile_dir
  local manifest
  local node_program
  profile_dir="$(dsh_tui_profile_dir "$profile")"
  manifest="$profile_dir/package.json"
  node_program="$(dsh_tui_resolve_node)" || return

  dsh_tui_prepare_family_profile "$repo_root" "$harness_root" "$profile"

  if dsh_tui_profile_manifest_is_ready "$manifest" \
    && dsh_tui_profile_manifest_links_current_repo "$manifest" "$repo_root" "$harness_root"; then
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
  "$node_program" "$harness_entry" plugin --profile "$profile" add \
    "$harness_root/packages/core/agent" \
    "$harness_root/packages/preset/agent-presets" \
    "$harness_root/packages/api/session-controller" \
    "$harness_root/packages/api/settings-controller" \
    "$harness_root/packages/api/workspace-controller" \
    "$harness_root/packages/util/brand" \
    "$harness_root/packages/code-runtime/code-runtime-worker-thread" \
    "$harness_root/packages/interaction/commands" \
    "$harness_root/packages/extensions/cordis-host-runner" \
    "$harness_root/packages/context/file-reference" \
    "$harness_root/packages/context/file-reference-local" \
    "$harness_root/packages/goal/goal" \
    "$harness_root/packages/host/directory-picker-browse" \
    "$harness_root/packages/runtime-diagnostics/invariants" \
    "$harness_root/packages/llm/llm" \
    "$harness_root/packages/core/session" \
    "$harness_root/packages/subagent/subagent" \
    "$harness_root/packages/subagent/tool-subagent" \
    "$harness_root/packages/workspace/workspace" \
    "$repo_root/packages/dsh-tui-protocol" \
    "$repo_root/packages/dsh-tui-server" \
    "$repo_root/packages/dsh-tui-embedded"

  dsh_tui_prepare_family_profile "$repo_root" "$harness_root" "$profile" stamp
  dsh_tui_require_profile "$profile" "$repo_root"
  printf 'DSH profile %s is ready.\n' "$profile" >&2
}
