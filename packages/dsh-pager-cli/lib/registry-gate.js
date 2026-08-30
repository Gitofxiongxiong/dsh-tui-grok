import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'

const EXACT_VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/

export function collectRegistryDependencies(manifestPaths) {
  const rows = new Map()
  const failures = []
  for (const manifestPath of manifestPaths) {
    let manifest
    try {
      manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
    } catch (error) {
      failures.push(`${manifestPath}: cannot read manifest (${firstLine(error)})`)
      continue
    }
    const owner = `${manifest.name ?? manifestPath}@${manifest.version ?? '?'}`
    const peerMeta = isRecord(manifest.peerDependenciesMeta) ? manifest.peerDependenciesMeta : {}
    const sections = [
      ['dependencies', manifest.dependencies],
      ['peerDependencies', manifest.peerDependencies],
    ]
    for (const [section, values] of sections) {
      if (!isRecord(values)) continue
      for (const [name, version] of Object.entries(values)) {
        if (section === 'peerDependencies' && peerMeta[name]?.optional === true) continue
        if (typeof version !== 'string' || !EXACT_VERSION.test(version)) {
          failures.push(`${owner}: ${section}.${name} must use an exact registry version, got ${JSON.stringify(version)}`)
          continue
        }
        const previous = rows.get(name)
        if (previous !== undefined && previous.version !== version) {
          failures.push(`${name}: conflicting exact versions ${previous.version} (${previous.owners.join(', ')}) and ${version} (${owner})`)
          continue
        }
        if (previous === undefined) rows.set(name, { name, version, owners: [owner] })
        else if (!previous.owners.includes(owner)) previous.owners.push(owner)
      }
    }
  }
  return { rows: [...rows.values()].sort((a, b) => a.name.localeCompare(b.name)), failures }
}

export function runRegistryDependencyGate(manifestPaths, options = {}) {
  const collected = collectRegistryDependencies(manifestPaths)
  const checks = []
  const failures = [...collected.failures]
  const runner = options.runner ?? npmView
  for (const row of collected.rows) {
    const result = runner(row.name, row.version, options)
    checks.push({ ...row, ok: result.ok, detail: result.detail })
    if (!result.ok) failures.push(`${row.name}@${row.version}: ${result.detail}`)
  }
  return { ok: failures.length === 0, checks, failures }
}

function npmView(name, version, options) {
  const npm = options.npmCommand ?? 'npm'
  const args = ['view', `${name}@${version}`, 'version', '--json']
  const commandArgs = npm.endsWith('.js') ? [npm, ...args] : args
  const command = npm.endsWith('.js') ? process.execPath : npm
  const result = spawnSync(command, commandArgs, {
    encoding: 'utf8',
    env: options.env ?? process.env,
    timeout: options.timeout ?? 30_000,
  })
  if (result.status !== 0) {
    return { ok: false, detail: firstLine(result.stderr || result.stdout || `npm view exited ${result.status ?? 1}`) }
  }
  let actual
  try {
    actual = JSON.parse(result.stdout)
  } catch {
    actual = result.stdout.trim().replace(/^"|"$/g, '')
  }
  const ok = actual === version
  return { ok, detail: ok ? `registry returned ${actual}` : `registry returned ${JSON.stringify(actual)}` }
}

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function firstLine(error) {
  return String(error?.message ?? error).split('\n')[0]
}
