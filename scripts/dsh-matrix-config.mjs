#!/usr/bin/env node
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const registry = JSON.parse(readFileSync(join(repoRoot, 'compat', 'dsh-support.json'), 'utf8'))
if (registry?.schemaVersion !== 1 || registry.versions === null || typeof registry.versions !== 'object') {
  throw new Error('invalid compat/dsh-support.json')
}

const rows = Object.entries(registry.versions).map(([version, entry]) => {
  const source = entry.distribution === 'source-only' ? 'source-only' : 'npm'
  return {
    version,
    family: entry.family,
    tag: entry.tag,
    commit: entry.commit,
    package_manager: entry.packageManager,
    checkout_env: checkoutEnvironment(version),
    checkout_dir: source === 'source-only' ? 'deepseek-harness-latest' : `dsh-matrix-${version}`,
    source,
    fixture: source === 'npm' ? `compat/fixtures/dsh-${version}` : '-',
    label: version.split('-').slice(1).join('-').replaceAll(/[^0-9A-Za-z]/g, ''),
  }
})

const [mode = '--json', value] = process.argv.slice(2)
if (mode === '--json') {
  process.stdout.write(`${JSON.stringify({ include: rows })}\n`)
} else if (mode === '--source-only-tsv') {
  const matches = rows.filter(row => row.source === 'source-only')
  if (matches.length !== 1) throw new Error(`expected one source-only matrix row, found ${matches.length}`)
  printTsv(matches[0])
} else if (mode === '--version-tsv') {
  const row = rows.find(candidate => candidate.version === value)
  if (row === undefined) throw new Error(`DSH ${value ?? '<missing>'} is absent from compat/dsh-support.json`)
  printTsv(row)
} else {
  throw new Error('usage: dsh-matrix-config.mjs [--json|--source-only-tsv|--version-tsv <version>]')
}

function printTsv(row) {
  process.stdout.write([
    row.version,
    row.family,
    row.tag,
    row.commit,
    row.package_manager,
    row.checkout_env,
    row.checkout_dir,
    row.source,
    row.fixture,
    row.label,
  ].join('\t') + '\n')
}

function checkoutEnvironment(version) {
  return `DSH_CHECKOUT_${version.toUpperCase().replace(/[^A-Z0-9]/g, '_')}_ROOT`
}
