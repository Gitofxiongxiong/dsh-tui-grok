#!/usr/bin/env node
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { copyPackageLicenses, LICENSE_FILES } from './copy-package-licenses.mjs'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const cli = join(repoRoot, 'packages/dsh-pager-cli')
const dir = mkdtempSync(join(tmpdir(), 'dsh-cli-pack-'))
const require = createRequire(import.meta.url)

function fail(message) {
  rmSync(dir, { recursive: true, force: true })
  console.error(`verify-cli-pack: ${message}`)
  process.exit(1)
}

function resolveNpmCli() {
  try {
    return require.resolve('npm/bin/npm-cli.js')
  } catch {
    // Node distributions normally place npm beside the executable.
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

copyPackageLicenses(cli)

const pack = spawnSync(process.execPath, [resolveNpmCli(), 'pack', '--pack-destination', dir], {
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
  fail(`npm pack printed no tarball:\n${pack.stdout}\n${pack.stderr}`)
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
  if (/^(?:link|workspace):/.test(String(spec))) {
    fail(`packed ${name} has local specifier ${spec}`)
  }
  if (String(spec).includes('alpha')) {
    fail(`packed ${name} contains alpha version ${spec}`)
  }
}
if (specs['@deepseek-ai/dsh'] !== '0.1.1-rc.2') {
  fail(`default DSH must pack as 0.1.1-rc.2, got ${specs['@deepseek-ai/dsh']}`)
}
for (const name of Object.keys(manifest.dependencies ?? {})) {
  if (name.startsWith('@dsh-pager-grok/runtime')) {
    fail(`CLI must delay-resolve family runtimes, found public dependency ${name}`)
  }
}
rmSync(dir, { recursive: true, force: true })
console.log(`verify-cli-pack: ${tarballLine} ok`)
