#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const registryPath = join(repoRoot, 'compat/dsh-support.json')
const outputPath = join(repoRoot, 'docs/DSH_SUPPORT.md')

export function renderSupportTable(registry) {
  const lines = [
    '# DSH 精确版本支持表',
    '',
    '> 本文件由 `compat/dsh-support.json` 生成；不要手工编辑。',
    '',
    '| DSH | Adapter family | Status | Distribution | Profile schema |',
    '|---|---|---|---|---:|',
  ]
  for (const [version, entry] of Object.entries(registry.versions)) {
    lines.push(`| \`${version}\` | \`${entry.family}\` | \`${entry.status}\` | \`${entry.distribution}\` | ${entry.profileSchema} |`)
  }
  lines.push('', '精确 tag、commit、package manager 与 runtime package 请查阅单一真源',
    '[`compat/dsh-support.json`](../compat/dsh-support.json)。', '')
  return lines.join('\n')
}

const expected = renderSupportTable(JSON.parse(readFileSync(registryPath, 'utf8')))
if (process.argv.includes('--check')) {
  const actual = readFileSync(outputPath, 'utf8')
  if (actual !== expected) {
    console.error(`generated DSH support table is stale: ${outputPath}`)
    process.exit(1)
  }
  console.log(`generated DSH support table is current: ${outputPath}`)
} else if (process.argv.includes('--write')) {
  writeFileSync(outputPath, expected)
  console.log(`wrote ${outputPath}`)
} else {
  process.stdout.write(expected)
}
