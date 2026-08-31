import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  collectRegistryDependencies,
  normalizeNpmViewVersion,
  runRegistryDependencyGate,
} from '../lib/registry-gate.js'

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true })
})

function manifest(value: object) {
  const root = mkdtempSync(join(tmpdir(), 'dsh-registry-gate-'))
  roots.push(root)
  mkdirSync(root, { recursive: true })
  const path = join(root, 'package.json')
  writeFileSync(path, JSON.stringify(value))
  return path
}

describe('registry dependency gate', () => {
  it('normalizes npm 11 scalar and npm 12 singleton-array version output', () => {
    expect(normalizeNpmViewVersion('"1.2.3"\n')).toBe('1.2.3')
    expect(normalizeNpmViewVersion('["1.2.3"]\n')).toBe('1.2.3')
    expect(normalizeNpmViewVersion('["1.2.3", "1.2.4"]\n')).toEqual(['1.2.3', '1.2.4'])
  })

  it('collects dependencies and required peers but excludes optional declarations', () => {
    const path = manifest({
      name: '@dsh-pager-grok/candidate',
      version: '0.1.0',
      dependencies: { exact: '1.2.3' },
      peerDependencies: { required: '2.0.0', optional: '3.0.0' },
      peerDependenciesMeta: { optional: { optional: true } },
      optionalDependencies: { native: '4.0.0' },
    })
    const result = collectRegistryDependencies([path])
    expect(result.failures).toEqual([])
    expect(result.rows.map(row => `${row.name}@${row.version}`)).toEqual(['exact@1.2.3', 'required@2.0.0'])
  })

  it('fails ranges/local specs and reports missing exact packages', () => {
    const path = manifest({
      name: '@dsh-pager-grok/candidate',
      version: '0.1.0',
      dependencies: { ranged: '^1.0.0', linked: 'workspace:*', missing: '9.9.9' },
    })
    const result = runRegistryDependencyGate([path], {
      runner: (name: string) => ({ ok: false, detail: `no registry row for ${name}` }),
    })
    expect(result.ok).toBe(false)
    expect(result.failures.join('\n')).toContain('must use an exact registry version')
    expect(result.failures.join('\n')).toContain('missing@9.9.9')
  })
})
