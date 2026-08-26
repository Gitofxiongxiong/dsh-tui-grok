#!/usr/bin/env node
// Check pager-platform-matrix.json against packages/native/<id>/package.json.
import { readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const matrix = JSON.parse(
  readFileSync(join(repoRoot, 'scripts/pager-platform-matrix.json'), 'utf8'),
)

function fail(message) {
  console.error(`verify-native-matrix: ${message}`)
  process.exit(1)
}

if (!Array.isArray(matrix.packages) || matrix.packages.length !== 5) {
  fail('expected exactly five native packages')
}

for (const pkg of matrix.packages) {
  const manifest = JSON.parse(readFileSync(join(repoRoot, pkg.dir, 'package.json'), 'utf8'))
  if (manifest.name !== pkg.npm) {
    fail(`${pkg.id}: name ${manifest.name} != ${pkg.npm}`)
  }
  if (manifest.bin) {
    fail(`${pkg.id}: native packages must not declare bin`)
  }
  if (!manifest.os?.includes(pkg.os) || !manifest.cpu?.includes(pkg.cpu)) {
    fail(`${pkg.id}: os/cpu metadata mismatch`)
  }
  if (pkg.libc && !manifest.libc?.includes(pkg.libc)) {
    fail(`${pkg.id}: libc metadata mismatch`)
  }
  const expectedFile = `bin/${pkg.bin}`
  if (!manifest.files?.includes(expectedFile)) {
    fail(`${pkg.id}: files must include ${expectedFile}`)
  }
}

console.log(`verify-native-matrix: ${matrix.packages.length} packages ok`)
