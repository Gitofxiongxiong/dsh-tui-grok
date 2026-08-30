#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release_root="$(mktemp -d "${TMPDIR:-/tmp}/dsh-pager-release-candidate.XXXXXX")"
artifacts="$release_root/artifacts"
install_root="$release_root/cold-install"
cold_home="$release_root/dsh-home"
logs="$release_root/logs"
mkdir -p "$artifacts" "$install_root" "$cold_home" "$logs" "$release_root/pnpm-store"

printf 'release candidate root (retained): %s\n' "$release_root"
printf 'No cleanup trap is installed; this isolated evidence directory is retained.\n'

registry_version="${DSH_RELEASE_REGISTRY_VERSION:-}"
registry_cli_tarball="${DSH_RELEASE_CLI_TARBALL:-}"
if [[ -n "$registry_version" || -n "$registry_cli_tarball" ]]; then
  if [[ ! "$registry_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$ ]]; then
    printf 'DSH_RELEASE_REGISTRY_VERSION must be an exact semver\n' >&2
    exit 1
  fi
  if [[ ! -f "$registry_cli_tarball" ]]; then
    printf 'DSH_RELEASE_CLI_TARBALL does not exist: %s\n' "$registry_cli_tarball" >&2
    exit 1
  fi
  registry_cli_tarball="$(readlink -f "$registry_cli_tarball")"
  publish_tarballs=(
    "$registry_cli_tarball"
    "@dsh-pager-grok/runtime-apiproxy-v1@$registry_version"
  )
  runtime_spec="@dsh-pager-grok/runtime-apiproxy-v1@$registry_version"
  printf 'registry mode: version=%s cli=%s\n' "$registry_version" "$registry_cli_tarball"
else
  corepack pnpm@11.20.0 --pm-on-fail=ignore --dir "$repo_root" run build:ts \
    >"$logs/build-ts.log" 2>&1
  node "$repo_root/scripts/pack-release-candidates.mjs" "$artifacts" \
    >"$logs/pack-candidates.log" 2>&1

  mapfile -t publish_tarballs < <(node --input-type=module - "$artifacts/release-candidates.json" <<'NODE'
import fs from 'node:fs'
const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
for (const artifact of manifest.artifacts) if (artifact.publish) console.log(artifact.tarball)
NODE
  )
  runtime_spec="$(node --input-type=module - "$artifacts/release-candidates.json" <<'NODE'
import fs from 'node:fs'
const manifest = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))
const runtime = manifest.artifacts.find(item => item.id === 'runtime-apiproxy-v1')
if (!runtime) process.exit(2)
process.stdout.write(runtime.tarball)
NODE
  )"
fi

cd "$install_root"
npm init --yes >"$logs/npm-init.log" 2>&1
cat >"$install_root/pnpm-workspace.yaml" <<'YAML'
packages:
  - .
allowBuilds:
  '@deepseek-ai/dsh-subprocess-local': true
  '@google/genai': false
  koffi: true
  node-pty: true
  protobufjs: false
YAML
corepack pnpm@11.20.0 --pm-on-fail=ignore --dir "$install_root" add --save-exact \
  --store-dir "$release_root/pnpm-store" \
  "${publish_tarballs[@]}" >"$logs/pnpm-install.log" 2>&1
corepack pnpm@11.20.0 --pm-on-fail=ignore --dir "$install_root" \
  list --depth Infinity --json >"$install_root/dependency-graph.json"
node "$repo_root/scripts/audit-cold-install.mjs" "$install_root" "$repo_root" \
  | tee "$logs/cold-audit.log"

cli="$install_root/node_modules/.bin/dsh-pager"
cli_manifest="$(readlink -f "$install_root/node_modules/@dsh-pager-grok/cli/package.json")"
native="$(node --input-type=module - "$cli_manifest" <<'NODE'
import { createRequire } from 'node:module'
import path from 'node:path'
const require = createRequire(process.argv[2])
const manifest = require.resolve('@dsh-pager-grok/native-linux-x64-gnu/package.json')
process.stdout.write(path.join(path.dirname(manifest), 'bin', 'dsh-pager'))
NODE
)"
if [[ ! -x "$native" ]]; then
  printf 'resolved native pager is not executable: %s\n' "$native" >&2
  exit 1
fi
dsh_entry="$(node --input-type=module - "$cli_manifest" <<'NODE'
import { createRequire } from 'node:module'
const require = createRequire(process.argv[2])
process.stdout.write(require.resolve('@deepseek-ai/dsh/lib/bin.js'))
NODE
)"
dsh_manifest="$(node --input-type=module - "$cli_manifest" <<'NODE'
import { createRequire } from 'node:module'
const require = createRequire(process.argv[2])
process.stdout.write(require.resolve('@deepseek-ai/dsh/package.json'))
NODE
)"
profile="dsh-pager-grok-apiproxy-v1"
export DSH_HOME="$cold_home"
export DSH_PAGER_DEV_MODE=1
export npm_config_store_dir="$release_root/pnpm-store"

# Prepare the cold family profile explicitly from the selected runtime spec,
# using the CLI's canonical policy/ownership helpers, then exercise the normal
# warm launcher path. Registry mode resolves runtime and native packages from
# npm while keeping the not-yet-published CLI as the audited Tag tarball.
export PATH="$install_root/node_modules/.bin:$PATH"
node "$dsh_entry" plugin --profile "$profile" list >"$logs/profile-init.log" 2>&1
node --input-type=module - \
  "$install_root/node_modules/@dsh-pager-grok/cli/lib/launcher.js" \
  "$profile" <<'NODE'
import { pathToFileURL } from 'node:url'
const launcher = await import(pathToFileURL(process.argv[2]))
launcher.ensureProfileBuildPolicy({ profile: process.argv[3] }, process.env)
NODE
node "$dsh_entry" plugin --profile "$profile" add "$runtime_spec" \
  >"$logs/profile-runtime-install.log" 2>&1
node --input-type=module - \
  "$install_root/node_modules/@dsh-pager-grok/cli/lib/launcher.js" \
  "$cli_manifest" \
  "$install_root/node_modules/@dsh-pager-grok/cli/lib/dsh-support.json" \
  "$dsh_manifest" \
  "$profile" <<'NODE'
import fs from 'node:fs'
import { pathToFileURL } from 'node:url'
const [, , launcherPath, cliPath, registryPath, dshPath, profile] = process.argv
const { writeProfileOwnership } = await import(pathToFileURL(launcherPath))
const cli = JSON.parse(fs.readFileSync(cliPath, 'utf8'))
const registry = JSON.parse(fs.readFileSync(registryPath, 'utf8'))
const dsh = JSON.parse(fs.readFileSync(dshPath, 'utf8'))
const support = registry.versions[dsh.version]
if (!support) throw new Error(`cold DSH ${dsh.version} is absent from packed registry`)
writeProfileOwnership(cli.version, {
  family: support.family,
  version: dsh.version,
  profileSchema: support.profileSchema,
  profile,
}, process.env)
NODE

"$cli" --hello 2>&1 | tee "$logs/hello.log"
"$cli" --list-sessions 2>&1 | tee "$logs/list.log"
"$cli" --load-only --new 2>&1 | tee "$logs/load.log"
"$cli" doctor --release 2>&1 | tee "$logs/doctor-release.log"
"$cli" --hello 2>&1 | tee "$logs/warm-hello.log"
env npm_config_offline=true npm_config_registry=http://127.0.0.1:9 \
  "$cli" --hello 2>&1 | tee "$logs/offline-hello.log"

DSH_HOME="$cold_home" python3 "$repo_root/scripts/pty-smoke.py" \
  --binary "$native" \
  --pager-arg=--new \
  --backend "$(command -v node)" \
  "--backend-arg=$dsh_entry" \
  "--backend-arg=--profile" \
  "--backend-arg=$profile" \
  --timeout 45 \
  >"$logs/pty.log" 2>&1

if rg -n 'workspace:|link:|0\.1\.2-alpha\.1|deepseek-harness|/home/leo/code' \
  "$install_root/pnpm-lock.yaml" "$cold_home/profiles/$profile/package.json"; then
  printf 'cold install/profile contains a forbidden source or specifier\n' >&2
  exit 1
fi

node --input-type=module - "$release_root" <<'NODE'
import fs from 'node:fs'
import path from 'node:path'
const root = process.argv[2]
const result = {
  schemaVersion: 1,
  result: 'passed',
  releaseRoot: root,
  installSource: process.env.DSH_RELEASE_REGISTRY_VERSION
    ? 'registry-native-runtime'
    : 'local-tarballs',
  checks: [
    process.env.DSH_RELEASE_REGISTRY_VERSION ? 'tag-cli-tarball' : 'pack',
    'cold-install',
    'dependency-graph',
    'doctor-release',
    'hello',
    'list',
    'load',
    'warm-hello',
    'offline-hello',
    'pty',
  ],
}
fs.writeFileSync(path.join(root, 'result.json'), `${JSON.stringify(result, null, 2)}\n`)
NODE

printf 'release candidate rehearsal passed: %s\n' "$release_root"
