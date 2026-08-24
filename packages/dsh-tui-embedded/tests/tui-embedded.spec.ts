/**
 * The bundle's substance is its patch file: the `dsh.bundle.patch` manifest
 * field must name a real, parseable patch list that composes over dsh-base.
 */

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import * as yaml from 'js-yaml'
import { applyEntryPatches, entryListSchema } from '@deepseek-ai/cordis-plugin-include'
import type { PatchOptions } from '@deepseek-ai/cordis-plugin-include'
import type { EntryOptions } from '@deepseek-ai/cordis-plugin-loader'
import * as TuiEmbedded from '../src/index.ts'

const packageRoot = fileURLToPath(new URL('..', import.meta.url))

function loadPatch(root: string): PatchOptions[] {
  const parsed: unknown = yaml.load(
    readFileSync(resolve(root, 'cordis.patch.yml'), 'utf8'),
    { schema: entryListSchema },
  )
  if (!Array.isArray(parsed)) throw new TypeError(`${root} patch must parse to a patch list`)
  return parsed as PatchOptions[]
}

function composedRows(): Map<string, EntryOptions> {
  const patches = loadPatch(packageRoot)
  const baseRows: EntryOptions[] = [{ id: 'hmr', name: '@deepseek-ai/cordis-plugin-hmr' }]
  const rows = applyEntryPatches(baseRows, patches, () => {})
  const byId = new Map<string, EntryOptions>()
  for (const row of rows) {
    if (typeof row.id === 'string') byId.set(row.id, row)
  }
  return byId
}

describe('dsh-tui-embedded bundle', () => {
  it('declares a parseable patch list through the dsh.bundle.patch manifest field', () => {
    const manifest = JSON.parse(
      readFileSync(resolve(packageRoot, 'package.json'), 'utf8'),
    ) as {
      dependencies?: Record<string, string>
      dsh?: { bundle?: { patch?: string } }
    }
    expect(manifest.dsh?.bundle?.patch).toBe('./cordis.patch.yml')
    expect(Array.isArray(loadPatch(packageRoot))).toBe(true)
    expect('default' in TuiEmbedded).toBe(false)
    expect(manifest.dependencies).toHaveProperty('@dsh-pager-grok/tui-server')
    expect(manifest.dependencies).toHaveProperty('@deepseek-ai/dsh-host-apiproxy')
    expect(manifest.dependencies).toHaveProperty('@deepseek-ai/dsh-session-projection-cache')
    expect(manifest.dependencies).not.toHaveProperty('@deepseek-ai/dsh-host-webserver')
  })

  it('composes the Embedded host rows over dsh-base without a Web transport', () => {
    const rows = composedRows()
    expect(rows.get('hmr')?.disabled).toBe(true)
    expect(rows.get('code-runtime')?.name).toBe('@deepseek-ai/dsh-code-runtime-worker-thread')
    expect(rows.get('storage')?.name).toBe('@deepseek-ai/dsh-storage')
    expect(rows.get('session-projection-cache')?.name).toBe('@deepseek-ai/dsh-session-projection-cache')
    expect(rows.get('workspace')?.name).toBe('@deepseek-ai/dsh-workspace')
    expect(rows.get('directory-picker')?.name).toBe('@deepseek-ai/dsh-host-directory-picker-browse')
    expect(rows.get('api-gateway')?.name).toBe('@deepseek-ai/dsh-host-apiproxy')
    expect(rows.get('session-list-projection-recovery')?.name)
      .toBe('@dsh-pager-grok/tui-session-projection-recovery')
    expect(rows.get('tui-server')?.name).toBe('@dsh-pager-grok/tui-server')
    expect(rows.has('webserver')).toBe(false)
    expect(rows.has('web-runtime')).toBe(false)
    expect(rows.has('headless-runner')).toBe(false)
    expect([...rows.keys()].some(id => id.startsWith('ui-'))).toBe(false)
  })
})
