#!/usr/bin/env node
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { copyPackageLicenses, LICENSE_FILES } from './copy-package-licenses.mjs'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const cli = join(repoRoot, 'packages/dsh-pager-cli')
const dir = mkdtempSync(join(tmpdir(), 'dsh-cli-pack-'))

function fail(message) {
  rmSync(dir, { recursive: true, force: true })
  console.error(`verify-cli-pack: ${message}`)
  process.exit(1)
}

function resolvePnpmCli() {
  const candidates = [
    join(cli, 'node_modules', 'pnpm', 'bin', 'pnpm.cjs'),
    join(repoRoot, 'node_modules', 'pnpm', 'bin', 'pnpm.cjs'),
  ]
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate
  }
  fail('cannot resolve pnpm/bin/pnpm.cjs from @dsh-pager-grok/cli; refusing PATH pnpm')
}

copyPackageLicenses(cli)

const pack = spawnSync(process.execPath, [resolvePnpmCli(), 'pack', '--pack-destination', dir], {
  cwd: cli,
  encoding: 'utf8',
})
if (pack.status !== 0) {
  fail(pack.stderr || pack.stdout)
}
const tarballLine = pack.stdout
  .split('\n')
  .map((line) => line.trim())
  .filter((line) => line.endsWith('.tgz'))
  .at(-1)
if (!tarballLine) {
  fail(`pnpm pack printed no tarball:\n${pack.stdout}\n${pack.stderr}`)
}
const tarballPath = tarballLine.startsWith('/') ? tarballLine : join(dir, tarballLine)
const listing = spawnSync('tar', ['-tzf', tarballPath], { encoding: 'utf8' })
if (listing.status !== 0) {
  fail(`tar -tzf ${tarballPath} failed: ${listing.stderr || listing.stdout}`)
}
const names = listing.stdout.split('\n').filter(Boolean)
const hasEntry = names.some(
  (name) => name.endsWith('bin/dsh-pager.js') || name.endsWith('lib/main.js'),
)
if (!hasEntry) {
  fail(`missing CLI entry: ${names.join(', ')}`)
}
for (const name of LICENSE_FILES) {
  if (!names.some((entry) => entry === name || entry.endsWith(`/${name}`))) {
    fail(`missing ${name}`)
  }
}
const packedJson = spawnSync('tar', ['-xOf', tarballPath, 'package/package.json'], {
  encoding: 'utf8',
})
if (packedJson.status !== 0) {
  fail(`unable to read packed package.json: ${packedJson.stderr}`)
}
const manifest = JSON.parse(packedJson.stdout)
const specs = {
  ...(manifest.dependencies ?? {}),
  ...(manifest.optionalDependencies ?? {}),
}
for (const [name, spec] of Object.entries(specs)) {
  if (String(spec).includes('workspace:')) {
    fail(`packed ${name} is still ${spec}`)
  }
}
const runtime = specs['@dsh-pager-grok/runtime']
if (runtime !== '0.1.0') {
  fail(`runtime must pack as 0.1.0, got ${runtime}`)
}
rmSync(dir, { recursive: true, force: true })
console.log(`verify-cli-pack: ${tarballLine} ok`)
