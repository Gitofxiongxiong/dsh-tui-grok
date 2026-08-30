#!/usr/bin/env node
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
import { afterEach, test } from 'node:test'
import { fileURLToPath } from 'node:url'

const scriptPath = fileURLToPath(new URL('./audit-cold-install.mjs', import.meta.url))
const temporaryRoots = []

afterEach(() => {
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true })
})

test('audits a pnpm isolated layout without a root native symlink', () => {
  const fixture = createFixture()

  assert.equal(existsSync(join(fixture.installRoot, 'node_modules', '@dsh-pager-grok', 'native-linux-x64-gnu')), false)
  const result = runAudit(fixture)

  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /cold install audit passed/)
  const audit = JSON.parse(readFileSync(join(fixture.installRoot, 'cold-audit.json'), 'utf8'))
  assert.deepEqual(audit.dshPackages, [{ name: '@deepseek-ai/dsh', versions: ['0.1.1-rc.2'] }])
})

test('rejects a CLI native dependency that resolves outside cold node_modules', () => {
  const fixture = createFixture({ externalNative: true })
  const result = runAudit(fixture)

  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /native-linux-x64-gnu escaped cold node_modules/)
})

function createFixture({ externalNative = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'dsh-cold-audit-test-'))
  temporaryRoots.push(root)
  const installRoot = join(root, 'cold-install')
  const forbiddenRoot = join(root, 'forbidden-repo')
  const nodeModules = join(installRoot, 'node_modules')
  const virtualStore = join(nodeModules, '.pnpm')
  const cliStoreModules = join(virtualStore, '@dsh-pager-grok+cli@0.2.0', 'node_modules')
  const cliReal = join(cliStoreModules, '@dsh-pager-grok', 'cli')
  const runtimeReal = join(
    virtualStore,
    '@dsh-pager-grok+runtime-apiproxy-v1@0.2.0',
    'node_modules',
    '@dsh-pager-grok',
    'runtime-apiproxy-v1',
  )
  const nativeReal = externalNative
    ? join(root, 'external-native')
    : join(
      virtualStore,
      '@dsh-pager-grok+native-linux-x64-gnu@0.2.0',
      'node_modules',
      '@dsh-pager-grok',
      'native-linux-x64-gnu',
    )

  mkdirSync(cliReal, { recursive: true })
  mkdirSync(runtimeReal, { recursive: true })
  mkdirSync(nativeReal, { recursive: true })
  mkdirSync(forbiddenRoot, { recursive: true })
  writeManifest(cliReal, {
    name: '@dsh-pager-grok/cli',
    version: '0.2.0',
    optionalDependencies: { '@dsh-pager-grok/native-linux-x64-gnu': '0.2.0' },
  })
  writeManifest(runtimeReal, {
    name: '@dsh-pager-grok/runtime-apiproxy-v1',
    version: '0.2.0',
    dependencies: { '@deepseek-ai/dsh': '0.1.1-rc.2' },
  })
  writeManifest(nativeReal, {
    name: '@dsh-pager-grok/native-linux-x64-gnu',
    version: '0.2.0',
  })

  linkDirectory(cliReal, join(nodeModules, '@dsh-pager-grok', 'cli'))
  linkDirectory(runtimeReal, join(nodeModules, '@dsh-pager-grok', 'runtime-apiproxy-v1'))
  linkDirectory(nativeReal, join(cliStoreModules, '@dsh-pager-grok', 'native-linux-x64-gnu'))

  writeFileSync(join(installRoot, 'pnpm-lock.yaml'), 'lockfileVersion: 9.0\n')
  writeFileSync(join(installRoot, 'dependency-graph.json'), `${JSON.stringify([{
    dependencies: {
      '@dsh-pager-grok/cli': packageNode('0.2.0'),
      '@dsh-pager-grok/runtime-apiproxy-v1': packageNode('0.2.0', {
        '@deepseek-ai/dsh': packageNode('0.1.1-rc.2'),
      }),
      '@dsh-pager-grok/native-linux-x64-gnu': packageNode('0.2.0'),
    },
  }], null, 2)}\n`)

  return { installRoot, forbiddenRoot }
}

function linkDirectory(target, path) {
  mkdirSync(dirname(path), { recursive: true })
  symlinkSync(relative(dirname(path), target), path, 'dir')
}

function packageNode(version, dependencies = {}) {
  return { version, dependencies }
}

function runAudit({ installRoot, forbiddenRoot }) {
  return spawnSync(process.execPath, [scriptPath, installRoot, forbiddenRoot], {
    encoding: 'utf8',
  })
}

function writeManifest(root, manifest) {
  writeFileSync(join(root, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`)
}
