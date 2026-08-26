#!/usr/bin/env node
import { mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const runtime = join(repoRoot, 'packages/dsh-pager-runtime')
const dir = mkdtempSync(join(tmpdir(), 'dsh-runtime-pack-'))

function fail(message) {
  rmSync(dir, { recursive: true, force: true })
  console.error(`verify-runtime-pack: ${message}`)
  process.exit(1)
}

const pack = spawnSync('npm', ['pack', '--pack-destination', dir], {
  cwd: runtime,
  encoding: 'utf8',
})
if (pack.status !== 0) {
  fail(pack.stderr || pack.stdout)
}
const tarball = pack.stdout.trim().split('\n').at(-1)
const extract = spawnSync('tar', ['-tzf', join(dir, tarball)], { encoding: 'utf8' })
const names = extract.stdout.split('\n').filter(Boolean)
if (!names.includes('package/lib/server/index.js')) {
  fail(`missing server entry: ${names.join(', ')}`)
}
if (!names.includes('package/lib/recovery/index.js')) {
  fail('missing recovery entry')
}
if (!names.includes('package/cordis.patch.yml')) {
  fail('missing cordis.patch.yml')
}
const packedJson = spawnSync('tar', ['-xOf', join(dir, tarball), 'package/package.json'], {
  encoding: 'utf8',
})
const manifest = JSON.parse(packedJson.stdout)
const deps = JSON.stringify(manifest.dependencies ?? {})
if (deps.includes('workspace:')) {
  fail('packed dependencies contain workspace:')
}
if (manifest.dsh?.bundle?.patch !== './cordis.patch.yml') {
  fail('missing dsh.bundle.patch')
}
rmSync(dir, { recursive: true, force: true })
console.log(`verify-runtime-pack: ${tarball} ok`)
