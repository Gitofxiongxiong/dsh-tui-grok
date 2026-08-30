#!/usr/bin/env node
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { runRegistryDependencyGate } from '../packages/dsh-pager-cli/lib/registry-gate.js'

const repoRoot = resolve(fileURLToPath(new URL('..', import.meta.url)))
const args = process.argv.slice(2)
const manifestPaths = (args.length > 0 ? args : [
  'packages/dsh-pager-runtime-apiproxy-v1/package.json',
  'packages/dsh-pager-cli/package.json',
]).map(path => resolve(repoRoot, path))

const result = runRegistryDependencyGate(manifestPaths)
for (const check of result.checks) {
  process.stdout.write(`${check.ok ? 'available' : 'missing'} ${check.name}@${check.version} (${check.detail})\n`)
}
if (result.failures.length > 0) {
  process.stderr.write(`registry dependency gate failed:\n${result.failures.map(item => `- ${item}`).join('\n')}\n`)
  process.exit(1)
}
process.stdout.write(`registry dependency gate passed (${result.checks.length} exact non-optional dependencies)\n`)
