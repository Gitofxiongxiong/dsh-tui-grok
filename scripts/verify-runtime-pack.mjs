#!/usr/bin/env node
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const runtime = join(repoRoot, 'packages/dsh-pager-runtime-apiproxy-v1')
const dir = mkdtempSync(join(tmpdir(), 'dsh-runtime-pack-'))
const require = createRequire(import.meta.url)

function fail(message) {
  rmSync(dir, { recursive: true, force: true })
  console.error(`verify-runtime-pack: ${message}`)
  process.exit(1)
}

function resolveNpmCli() {
  try {
    return require.resolve('npm/bin/npm-cli.js')
  } catch {
    // Node ships npm beside the executable. Never spawn PATH `npm` (Windows .cmd).
  }
  const prefix = dirname(process.execPath)
  const candidates = [
    join(prefix, 'node_modules', 'npm', 'bin', 'npm-cli.js'),
    join(prefix, '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js'),
  ]
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate
  }
  fail('cannot resolve npm/bin/npm-cli.js next to node; refusing PATH npm')
}

const pack = spawnSync(process.execPath, [resolveNpmCli(), 'pack', '--pack-destination', dir], {
  cwd: runtime,
  encoding: 'utf8',
})
if (pack.status !== 0) {
  fail(pack.stderr || pack.stdout)
}
const tarball = pack.stdout.trim().split('\n').at(-1)
const extract = spawnSync('tar', ['-tzf', join(dir, tarball)], { encoding: 'utf8' })
const names = extract.stdout.split('\n').filter(Boolean)
if (!names.includes('package/lib/server-entry.js')) {
  fail(`missing server entry: ${names.join(', ')}`)
}
if (names.some(name => name.includes('controllers-v2'))) {
  fail('controllers-v2 files leaked into apiproxy-v1 runtime pack')
}
if (names.some(name => name.includes('/recovery'))) fail('retired recovery files leaked into runtime pack')
if (!names.includes('package/cordis.patch.yml')) {
  fail('missing cordis.patch.yml')
}
for (const name of ['LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE']) {
  if (!names.includes(`package/${name}`)) {
    fail(`missing ${name}`)
  }
}
const packedJson = spawnSync('tar', ['-xOf', join(dir, tarball), 'package/package.json'], {
  encoding: 'utf8',
})
const manifest = JSON.parse(packedJson.stdout)
for (const [name, specifier] of Object.entries(manifest.dependencies ?? {})) {
  if (/^(?:link|workspace):/.test(String(specifier))) {
    fail(`packed dependency ${name} contains local specifier ${specifier}`)
  }
  if (String(specifier).includes('alpha')) {
    fail(`packed dependency ${name} contains alpha version ${specifier}`)
  }
  if (name.startsWith('@deepseek-ai/dsh') && specifier !== '0.1.1-rc.2') {
    fail(`packed dependency ${name} must be exact rc.2, got ${specifier}`)
  }
}
if (manifest.name !== '@dsh-pager-grok/runtime-apiproxy-v1') {
  fail(`unexpected runtime package name ${manifest.name}`)
}
if (manifest.dshPagerGrok?.adapterFamily !== 'apiproxy-v1' || manifest.dshPagerGrok?.profileSchema !== 1) {
  fail('missing apiproxy-v1 runtime metadata')
}
if (manifest.dsh?.bundle?.patch !== './cordis.patch.yml') {
  fail('missing dsh.bundle.patch')
}
const jsNames = names.filter(name => name.endsWith('.js'))
const packedJs = spawnSync('tar', ['-xOf', join(dir, tarball), ...jsNames], { encoding: 'utf8' })
if (packedJs.status !== 0) fail(`cannot inspect packed JS: ${packedJs.stderr}`)
for (const forbidden of ['controllers-v2', '0.1.2-alpha.1', '@deepseek-ai/dsh-session/chunk-rows']) {
  if (packedJs.stdout.includes(forbidden)) fail(`packed JS contains forbidden ${forbidden}`)
}
rmSync(dir, { recursive: true, force: true })
console.log(`verify-runtime-pack: ${tarball} ok`)
