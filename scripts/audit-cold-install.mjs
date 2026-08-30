#!/usr/bin/env node
import { readFileSync, realpathSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

const installRoot = resolve(process.argv[2] ?? '')
const forbiddenRoot = resolve(process.argv[3] ?? '')
if (!process.argv[2] || !process.argv[3]) {
  throw new Error('usage: audit-cold-install.mjs <install-root> <forbidden-repo-root>')
}

const lockPath = join(installRoot, 'pnpm-lock.yaml')
const lockText = readFileSync(lockPath, 'utf8')
for (const forbidden of ['workspace:', 'link:', '0.1.2-alpha.1', forbiddenRoot, 'deepseek-harness-latest']) {
  if (lockText.includes(forbidden)) throw new Error(`cold lock contains forbidden ${forbidden}`)
}

const graphPath = join(installRoot, 'dependency-graph.json')
const parsedGraph = JSON.parse(readFileSync(graphPath, 'utf8'))
const graph = Array.isArray(parsedGraph) ? parsedGraph[0] : parsedGraph
const seen = new Set()
const versions = new Map()
walk(graph.dependencies ?? {})

for (const name of [
  '@dsh-pager-grok/native-linux-x64-gnu',
  '@dsh-pager-grok/runtime-apiproxy-v1',
  '@dsh-pager-grok/cli',
  '@deepseek-ai/dsh',
]) {
  if (!versions.has(name)) throw new Error(`cold graph is missing ${name}`)
}
for (const [name, values] of versions) {
  if (name === '@deepseek-ai/dsh' || name.startsWith('@deepseek-ai/dsh-')) {
    for (const version of values) {
      if (version !== '0.1.1-rc.2') throw new Error(`cold graph contains ${name}@${version}`)
    }
  }
}

for (const name of [
  '@dsh-pager-grok/native-linux-x64-gnu',
  '@dsh-pager-grok/runtime-apiproxy-v1',
  '@dsh-pager-grok/cli',
]) {
  const path = join(installRoot, 'node_modules', ...name.split('/'))
  const real = realpathSync(path)
  if (!real.startsWith(join(installRoot, 'node_modules'))) {
    throw new Error(`${name} escaped cold node_modules: ${real}`)
  }
  const manifest = JSON.parse(readFileSync(join(path, 'package.json'), 'utf8'))
  for (const section of ['dependencies', 'peerDependencies', 'optionalDependencies']) {
    for (const [dependency, specifier] of Object.entries(manifest[section] ?? {})) {
      if (/^(?:link|workspace):/.test(String(specifier))) {
        throw new Error(`${name} ${section}.${dependency} contains ${specifier}`)
      }
      if (String(specifier).includes('alpha')) {
        throw new Error(`${name} ${section}.${dependency} contains ${specifier}`)
      }
    }
  }
}

const result = {
  schemaVersion: 1,
  installRoot,
  packagesVisited: seen.size,
  packageManager: 'pnpm@11.20.0',
  dshPackages: [...versions]
    .filter(([name]) => name === '@deepseek-ai/dsh' || name.startsWith('@deepseek-ai/dsh-'))
    .map(([name, values]) => ({ name, versions: [...values].sort() }))
    .sort((a, b) => a.name.localeCompare(b.name)),
  localCandidatePackages: [...versions]
    .filter(([name]) => name.startsWith('@dsh-pager-grok/'))
    .map(([name, values]) => ({ name, versions: [...values].sort() }))
    .sort((a, b) => a.name.localeCompare(b.name)),
}
writeFileSync(join(installRoot, 'cold-audit.json'), `${JSON.stringify(result, null, 2)}\n`)
process.stdout.write(`cold install audit passed (${seen.size} graph packages; ${result.dshPackages.length} DSH packages exact rc.2)\n`)

function walk(dependencies) {
  for (const [name, value] of Object.entries(dependencies)) {
    if (value === null || typeof value !== 'object') continue
    const key = `${name}@${value.version ?? '?'}`
    if (seen.has(key)) continue
    seen.add(key)
    if (!versions.has(name)) versions.set(name, new Set())
    versions.get(name).add(value.version ?? '?')
    walk(value.dependencies ?? {})
  }
}
