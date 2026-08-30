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

describe('@dsh-pager-grok/runtime-apiproxy-v1 bundle', () => {
  it('pins the rc.2 graph and mounts only the ApiProxy family server', () => {
    const manifest = JSON.parse(readFileSync(resolve(packageRoot, 'package.json'), 'utf8')) as {
      dependencies?: Record<string, string>
      dsh?: { bundle?: { patch?: string } }
      dshPagerGrok?: { adapterFamily?: string; profileSchema?: number }
    }
    expect(manifest.dsh?.bundle?.patch).toBe('./cordis.patch.yml')
    expect(manifest.dshPagerGrok).toMatchObject({ adapterFamily: 'apiproxy-v1', profileSchema: 1 })
    for (const [name, version] of Object.entries(manifest.dependencies ?? {})) {
      expect(version, name).not.toMatch(/workspace:|link:|alpha/)
      if (name.startsWith('@deepseek-ai/dsh')) expect(version, name).toBe('0.1.1-rc.2')
    }

    const rows = new Map<string, EntryOptions>()
    for (const row of applyEntryPatches(
      [
        { id: 'hmr', name: '@deepseek-ai/cordis-plugin-hmr' },
        { id: 'tool-bash', name: '@deepseek-ai/dsh-tool-bash' },
      ],
      loadPatch(),
      () => {},
    )) {
      if (typeof row.id === 'string') rows.set(row.id, row)
    }
    expect(rows.get('hmr')?.disabled).toBe(true)
    expect(rows.get('api-gateway')?.name).toBe('@deepseek-ai/dsh-host-apiproxy')
    expect(rows.get('session-projection-cache')?.name)
      .toBe('@deepseek-ai/dsh-session-projection-cache')
    expect(rows.get('tui-server')?.name).toBe('@dsh-pager-grok/runtime-apiproxy-v1/server')
    expect(rows.has('session-controller')).toBe(false)
    expect(rows.has('settings-controller')).toBe(false)
    expect(rows.has('workspace-controller')).toBe(false)
    expect(rows.has('session-list-projection-recovery')).toBe(false)
    expect(rows.has('webserver')).toBe(false)
  })
})
