import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import * as yaml from 'js-yaml'
import { applyEntryPatches, entryListSchema } from '@deepseek-ai/cordis-plugin-include'
import type { PatchOptions } from '@deepseek-ai/cordis-plugin-include'
import type { EntryOptions } from '@deepseek-ai/cordis-plugin-loader'

const packageRoot = fileURLToPath(new URL('..', import.meta.url))

function loadPatch(): PatchOptions[] {
  const parsed: unknown = yaml.load(
    readFileSync(resolve(packageRoot, 'cordis.patch.yml'), 'utf8'),
    { schema: entryListSchema },
  )
  if (!Array.isArray(parsed)) throw new TypeError('runtime patch must parse to a list')
  return parsed as PatchOptions[]
}

describe('@dsh-pager-grok/runtime bundle', () => {
  it('declares a parseable dsh.bundle.patch with runtime subpath plugins', () => {
    const manifest = JSON.parse(readFileSync(resolve(packageRoot, 'package.json'), 'utf8')) as {
      dependencies?: Record<string, string>
      dsh?: { bundle?: { patch?: string } }
    }
    expect(manifest.dsh?.bundle?.patch).toBe('./cordis.patch.yml')
    expect(Object.values(manifest.dependencies ?? {}).some((value) => value.includes('workspace:'))).toBe(
      false,
    )
    const rows = new Map<string, EntryOptions>()
    for (const row of applyEntryPatches(
      [{ id: 'hmr', name: '@deepseek-ai/cordis-plugin-hmr' }],
      loadPatch(),
      () => {},
    )) {
      if (typeof row.id === 'string') rows.set(row.id, row)
    }
    expect(rows.get('hmr')?.disabled).toBe(true)
    expect(rows.get('session-list-projection-recovery')?.name).toBe(
      '@dsh-pager-grok/runtime/recovery',
    )
    expect(rows.get('tui-server')?.name).toBe('@dsh-pager-grok/runtime/server')
    expect(rows.has('webserver')).toBe(false)
  })
})
