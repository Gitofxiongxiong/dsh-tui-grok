#!/usr/bin/env node

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const registryPath = join(repoRoot, 'compat', 'dsh-support.json')
const runtimeManifestPaths = [
  join(repoRoot, 'packages', 'dsh-pager-runtime', 'package.json'),
  join(repoRoot, 'packages', 'dsh-pager-runtime-apiproxy-v1', 'package.json'),
]
const cliManifestPath = join(repoRoot, 'packages', 'dsh-pager-cli', 'package.json')
const registryConsumerPaths = [
  join(repoRoot, 'packages', 'dsh-pager-cli', 'lib', 'launcher.js'),
  join(repoRoot, 'packages', 'dsh-pager-cli', 'lib', 'main.js'),
  join(repoRoot, 'scripts', 'dsh-tui-common.sh'),
]
const failures = []
const reports = []

const VERSION_FIELDS = [
  'family',
  'tag',
  'commit',
  'packageManager',
  'runtimePackage',
  'profileSchema',
  'status',
  'distribution',
]
const FAMILIES = new Set(['apiproxy-v1', 'controllers-v2'])
const STATUSES = new Set([
  'supported',
  'maintenance',
  'candidate',
  'experimental',
  'unsupported',
])
const DISTRIBUTIONS = new Set(['npm', 'source-only', 'npm-candidate'])
const EXACT_VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/
const COMMIT = /^[0-9a-f]{40}$/

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function readJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'))
  } catch (error) {
    failures.push(`${label}: cannot read valid JSON from ${path}: ${error.message}`)
    return undefined
  }
}

function checkKeys(value, required, label) {
  if (!isRecord(value)) {
    failures.push(`${label}: expected an object`)
    return false
  }
  for (const key of required) {
    if (!Object.hasOwn(value, key)) failures.push(`${label}: missing required field ${key}`)
  }
  for (const key of Object.keys(value)) {
    if (!required.includes(key)) failures.push(`${label}: unknown field ${key}`)
  }
  return true
}

function validateRegistry(value) {
  if (!checkKeys(value, ['schemaVersion', 'versions'], 'registry')) return new Map()
  if (value.schemaVersion !== 1) {
    failures.push(`registry.schemaVersion: expected 1, got ${JSON.stringify(value.schemaVersion)}`)
  }
  if (!isRecord(value.versions) || Object.keys(value.versions).length === 0) {
    failures.push('registry.versions: expected a non-empty object')
    return new Map()
  }

  const entries = new Map()
  const tags = new Map()
  const commits = new Map()
  for (const [version, entry] of Object.entries(value.versions)) {
    const label = `registry.versions[${JSON.stringify(version)}]`
    if (!EXACT_VERSION.test(version)) failures.push(`${label}: key is not an exact version`)
    if (!checkKeys(entry, VERSION_FIELDS, label)) continue
    entries.set(version, entry)

    if (!FAMILIES.has(entry.family)) {
      failures.push(`${label}.family: unsupported value ${JSON.stringify(entry.family)}`)
    }
    if (entry.tag !== `dsh-v${version}`) {
      failures.push(`${label}.tag: expected ${JSON.stringify(`dsh-v${version}`)}, got ${JSON.stringify(entry.tag)}`)
    }
    if (typeof entry.tag === 'string') {
      const previous = tags.get(entry.tag)
      if (previous !== undefined) failures.push(`${label}.tag: duplicates version ${previous}`)
      tags.set(entry.tag, version)
    }
    if (typeof entry.commit !== 'string' || !COMMIT.test(entry.commit)) {
      failures.push(`${label}.commit: expected a lowercase 40-character Git commit`)
    } else {
      const previous = commits.get(entry.commit)
      if (previous !== undefined) failures.push(`${label}.commit: duplicates version ${previous}`)
      commits.set(entry.commit, version)
    }
    if (typeof entry.packageManager !== 'string' || !/^pnpm@\d+\.\d+\.\d+$/.test(entry.packageManager)) {
      failures.push(`${label}.packageManager: expected exact pnpm@x.y.z`)
    }
    if (typeof entry.runtimePackage !== 'string' || !entry.runtimePackage.startsWith('@dsh-pager-grok/runtime-')) {
      failures.push(`${label}.runtimePackage: expected a family runtime package`)
    }
    if (!Number.isSafeInteger(entry.profileSchema) || entry.profileSchema < 1) {
      failures.push(`${label}.profileSchema: expected a positive safe integer`)
    }
    if (!STATUSES.has(entry.status)) {
      failures.push(`${label}.status: unsupported value ${JSON.stringify(entry.status)}`)
    }
    if (!DISTRIBUTIONS.has(entry.distribution)) {
      failures.push(`${label}.distribution: unsupported value ${JSON.stringify(entry.distribution)}`)
    }
  }
  return entries
}

function checkoutEnvironment(version) {
  return `DSH_CHECKOUT_${version.toUpperCase().replace(/[^A-Z0-9]/g, '_')}_ROOT`
}

function git(checkout, ...args) {
  return execFileSync('git', ['-C', checkout, ...args], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim()
}

function validateCheckout(version, entry) {
  const environment = checkoutEnvironment(version)
  const checkout = process.env[environment]
  if (checkout === undefined || checkout.trim() === '') {
    reports.push(`SKIP ${version} checkout: ${environment} is not set`)
    return
  }
  if (!existsSync(checkout)) {
    failures.push(`${version} checkout: ${environment} points to missing path ${checkout}`)
    return
  }

  try {
    const head = git(checkout, 'rev-parse', 'HEAD')
    if (head !== entry.commit) {
      failures.push(`${version} checkout HEAD: expected ${entry.commit}, got ${head}`)
    }
    const tagCommit = git(checkout, 'rev-parse', `refs/tags/${entry.tag}^{commit}`)
    if (tagCommit !== entry.commit) {
      failures.push(`${version} tag ${entry.tag}: expected ${entry.commit}, got ${tagCommit}`)
    }
  } catch (error) {
    failures.push(`${version} checkout Git metadata: ${error.stderr?.trim() || error.message}`)
  }

  const rootManifest = readJson(join(checkout, 'package.json'), `${version} root manifest`)
  if (rootManifest !== undefined && rootManifest.packageManager !== entry.packageManager) {
    failures.push(`${version} packageManager: expected ${entry.packageManager}, got ${JSON.stringify(rootManifest.packageManager)}`)
  }
  const cliManifest = readJson(
    join(checkout, 'apps', 'cli', 'package.json'),
    `${version} CLI manifest`,
  )
  if (cliManifest !== undefined && cliManifest.version !== version) {
    failures.push(`${version} CLI version: expected ${version}, got ${JSON.stringify(cliManifest.version)}`)
  }
  reports.push(`checked ${version} checkout: ${checkout}`)
}

function validateRuntimeManifest(path, entries) {
  const manifest = readJson(path, `runtime manifest ${path}`)
  if (!isRecord(manifest)) return
  const label = String(manifest.name ?? path)
  const dependencySections = [
    'dependencies',
    'devDependencies',
    'peerDependencies',
    'optionalDependencies',
  ]
  const deepseekPackages = new Map()
  for (const section of dependencySections) {
    const dependencies = manifest[section]
    if (dependencies === undefined) continue
    if (!isRecord(dependencies)) {
      failures.push(`${label} ${section}: expected an object`)
      continue
    }
    for (const [name, specifier] of Object.entries(dependencies)) {
      if (!name.startsWith('@deepseek-ai/')) continue
      let actualVersion = specifier
      if (typeof specifier === 'string' && specifier.startsWith('link:') && manifest.private === true) {
        const linkedManifest = readJson(
          join(resolve(dirname(path), specifier.slice('link:'.length)), 'package.json'),
          `${label} linked dependency ${name}`,
        )
        if (linkedManifest?.name !== name || !EXACT_VERSION.test(linkedManifest?.version ?? '')) {
          failures.push(`${label} ${section}.${name}: invalid linked package identity/version`)
          continue
        }
        actualVersion = linkedManifest.version
      } else if (typeof specifier !== 'string' || !EXACT_VERSION.test(specifier)) {
        failures.push(`${label} ${section}.${name}: expected an exact version or private source link, got ${JSON.stringify(specifier)}`)
        continue
      }
      const previous = deepseekPackages.get(name)
      if (previous !== undefined && previous.specifier !== actualVersion) {
        failures.push(`${label} ${name}: ${previous.section} resolves ${previous.specifier}, ${section} resolves ${actualVersion}`)
      }
      deepseekPackages.set(name, { section, specifier: actualVersion })
    }
  }

  const dshVersions = new Set()
  for (const [name, { specifier }] of deepseekPackages) {
    if (name === '@deepseek-ai/dsh' || name.startsWith('@deepseek-ai/dsh-')) {
      dshVersions.add(specifier)
    }
  }
  if (dshVersions.size === 0) {
    failures.push(`${label}: no exact @deepseek-ai/dsh or @deepseek-ai/dsh-* dependency found`)
    return
  }
  if (dshVersions.size > 1) {
    failures.push(`${label} DSH packages disagree: ${[...dshVersions].sort().join(', ')}`)
  }
  for (const version of [...dshVersions].sort()) {
    const support = entries.get(version)
    if (support === undefined) {
      failures.push(`${label} DSH version ${version}: missing from compat/dsh-support.json`)
      continue
    }
    if (!DISTRIBUTIONS.has(support.distribution)) {
      failures.push(`${label} DSH version ${version}: invalid registry distribution ${JSON.stringify(support.distribution)}`)
      continue
    }
    if (manifest.dshPagerGrok?.adapterFamily !== support.family) {
      failures.push(`${label}: adapter family ${JSON.stringify(manifest.dshPagerGrok?.adapterFamily)} disagrees with ${version} registry family ${support.family}`)
    }
    if (support.distribution === 'source-only') {
      if (manifest.private !== true) failures.push(`${label}: source-only runtime must be private`)
    } else if (manifest.name !== support.runtimePackage) {
      failures.push(`${label}: npm registry runtime for ${version} must be ${support.runtimePackage}`)
    }
    reports.push(`${label} DSH ${version}: registry distribution=${support.distribution}`)
  }
  if (manifest.private !== true) {
    for (const [name, { specifier }] of deepseekPackages) {
      if (specifier.includes('alpha')) failures.push(`${label}: publishable dependency ${name}@${specifier} is alpha`)
    }
  }
  reports.push(`checked ${deepseekPackages.size} exact @deepseek-ai/* declarations in ${label}`)
}

function validateRegistryConsumers(entries) {
  for (const path of registryConsumerPaths) {
    let source
    try {
      source = readFileSync(path, 'utf8')
    } catch (error) {
      failures.push(`registry consumer: cannot read ${path}: ${error.message}`)
      continue
    }
    for (const version of entries.keys()) {
      if (source.includes(version)) {
        failures.push(`registry consumer ${path}: hard-codes ${version} instead of reading compat/dsh-support.json`)
      }
    }
  }
  const launcher = readFileSync(registryConsumerPaths[0], 'utf8')
  if (!launcher.includes('dsh-support.json')) {
    failures.push('CLI launcher: does not locate the canonical/packed support registry')
  }
  const common = readFileSync(registryConsumerPaths[2], 'utf8')
  if (!common.includes('compat/dsh-support.json')) {
    failures.push('dsh-tui-common.sh: does not read compat/dsh-support.json')
  }
  const cliManifest = readJson(cliManifestPath, 'CLI manifest')
  if (isRecord(cliManifest) && !String(cliManifest.scripts?.prepack ?? '').includes('copy-support-registry.mjs')) {
    failures.push('CLI manifest: prepack does not derive its bundled support registry from the canonical file')
  }
  if (isRecord(cliManifest)) {
    const dshVersion = cliManifest.dependencies?.['@deepseek-ai/dsh']
    const support = entries.get(dshVersion)
    if (support === undefined || support.distribution !== 'npm') {
      failures.push(`CLI manifest: @deepseek-ai/dsh must be an exact registry npm version, got ${JSON.stringify(dshVersion)}`)
    }
    for (const [name, specifier] of Object.entries(cliManifest.dependencies ?? {})) {
      if (String(specifier).includes('alpha') || /^(?:link|workspace):/.test(String(specifier))) {
        failures.push(`CLI manifest: public dependency ${name}@${specifier} is not registry publishable`)
      }
    }
  }
  reports.push(`checked ${registryConsumerPaths.length} CLI/script registry consumers for version literals`)
}

const registry = readJson(registryPath, 'support registry')
const entries = registry === undefined ? new Map() : validateRegistry(registry)
for (const path of runtimeManifestPaths) validateRuntimeManifest(path, entries)
validateRegistryConsumers(entries)
for (const [version, entry] of entries) validateCheckout(version, entry)

for (const report of reports) console.log(`check-dsh-support: ${report}`)
if (failures.length > 0) {
  console.error(`check-dsh-support: FAILED (${failures.length} difference${failures.length === 1 ? '' : 's'})`)
  for (const failure of failures) console.error(`- ${failure}`)
  process.exitCode = 1
} else {
  console.log(`check-dsh-support: ok (${entries.size} registry versions)`)
}
