import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const fixtureDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = process.env.DSH_PAGER_GROK_ROOT ?? resolve(fixtureDir, '../../..')
const requireFromFixture = createRequire(import.meta.url)
const expectedVersion = process.env.DSH_COMPAT_VERSION ?? '0.1.1-rc.2'

for (const packageName of ['@deepseek-ai/dsh', '@deepseek-ai/dsh-host-apiproxy']) {
  const manifestPath = requireFromFixture.resolve(`${packageName}/package.json`)
  const manifest = requireFromFixture(manifestPath)
  assert.equal(manifest.version, expectedVersion, `${packageName} must resolve the selected exact version`)
  assert.ok(manifestPath.startsWith(fixtureDir), `${packageName} escaped the isolated fixture`)
  process.stdout.write(`${packageName}@${manifest.version}: ${manifestPath}\n`)
}

const runtimeLib = resolve(repoRoot, 'packages/dsh-pager-runtime-apiproxy-v1/lib')
const adapterPath = resolve(runtimeLib, 'server/adapters/apiproxy-v1/backend.js')
const corePath = resolve(runtimeLib, 'server/core/serve.js')
const [{ ApiProxyV1Backend, resolveApiProxyV1Runtime }, { serve }] = await Promise.all([
  import(pathToFileURL(adapterPath).href),
  import(pathToFileURL(corePath).href),
])
assert.equal(typeof ApiProxyV1Backend, 'function')
assert.equal(typeof serve, 'function')
assert.equal(typeof resolveApiProxyV1Runtime(requireFromFixture).toFetchHandler, 'function')
await import('./index.js')
process.stdout.write(`${expectedVersion} fixture build/import checks passed\n`)
