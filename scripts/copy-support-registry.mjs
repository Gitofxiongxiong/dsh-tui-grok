#!/usr/bin/env node
import { copyFileSync, mkdirSync, readFileSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = fileURLToPath(new URL('..', import.meta.url))
const source = join(repoRoot, 'compat', 'dsh-support.json')
const packageRoot = resolve(process.argv[2] ?? join(repoRoot, 'packages', 'dsh-pager-cli'))
const destination = join(packageRoot, 'lib', 'dsh-support.json')

if (process.argv.includes('--clean')) {
  rmSync(destination, { force: true })
  console.log(`copy-support-registry: removed derived ${destination}`)
  process.exit(0)
}

const registry = JSON.parse(readFileSync(source, 'utf8'))
if (registry?.schemaVersion !== 1 || registry?.versions === undefined) {
  throw new Error(`invalid canonical support registry: ${source}`)
}
mkdirSync(dirname(destination), { recursive: true })
copyFileSync(source, destination)
console.log(`copy-support-registry: ${source} -> ${destination}`)
