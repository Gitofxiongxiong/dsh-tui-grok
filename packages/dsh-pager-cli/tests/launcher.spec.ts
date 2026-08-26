import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  BUNDLE,
  PROFILE,
  commandName,
  hasUserBackend,
  helpText,
  needBundle,
  productBackendArgs,
  resolvePagerBinary,
  userBackendKind,
} from '../lib/launcher.js'
import { enginesSatisfied, nativeSpec } from '../lib/platform.js'

describe('cli argv', () => {
  it('classifies launcher commands from argv[2]', () => {
    expect(commandName([])).toBe('run')
    expect(commandName(['--new'])).toBe('run')
    expect(commandName(['doctor'])).toBe('doctor')
    expect(commandName(['--help'])).toBe('help')
    expect(commandName(['-v'])).toBe('version')
    expect(commandName(['repair'])).toBe('repair')
  })

  it('treats --backend and DSH_TUI_SERVER as user backend overrides', () => {
    expect(hasUserBackend(['--new'])).toBe(false)
    expect(hasUserBackend(['--backend', 'node'])).toBe(true)
    expect(hasUserBackend([], { DSH_TUI_SERVER: 'node ./mock.mjs' })).toBe(true)
    expect(hasUserBackend([], { DSH_TUI_SERVER: '' })).toBe(false)
    expect(userBackendKind([], { DSH_TUI_SERVER: '' })).toBe('blank')
    expect(userBackendKind([], { DSH_TUI_SERVER: '  \t' })).toBe('blank')
    expect(userBackendKind([], { DSH_TUI_SERVER: 'node ./mock.mjs' })).toBe('env')
    expect(userBackendKind(['--backend', 'node'])).toBe('argv')
  })

  it('ignores DSH_PAGER_BIN unless DSH_PAGER_DEV_MODE=1', () => {
    const override = join(tmpdir(), 'not-the-pager')
    const product = resolvePagerBinary({ env: { ...process.env, DSH_PAGER_BIN: override } })
    expect(product).not.toBe(override)
    const forced = resolvePagerBinary({
      env: { ...process.env, DSH_PAGER_BIN: override, DSH_PAGER_DEV_MODE: '1' },
    })
    expect(forced).toBe(override)
  })

  it('injects node + bin.js + profile', () => {
    const args = productBackendArgs({ node: '/usr/bin/node', binJs: '/opt/dsh/lib/bin.js' })
    expect(args).toEqual([
      '--backend',
      '/usr/bin/node',
      '--backend-arg',
      '/opt/dsh/lib/bin.js',
      '--backend-arg',
      '--profile',
      '--backend-arg',
      PROFILE,
    ])
  })

  it('documents pager flags in launcher help', () => {
    const text = helpText()
    expect(text).toContain('--list-sessions')
    expect(text).toContain('--backend-arg')
    expect(text).toContain(PROFILE)
  })
})

describe('warm skip', () => {
  it('needs a bundle when the profile is missing or the version differs', () => {
    const missing = { DSH_HOME: join(process.cwd(), '.no-such-dsh-home') }
    expect(needBundle(missing, '0.1.0')).toBe(true)

    const home = mkdtempSync(join(tmpdir(), 'dsh-pager-cli-'))
    const profile = join(home, 'profiles', PROFILE)
    mkdirSync(join(profile, 'node_modules', BUNDLE), { recursive: true })
    writeFileSync(
      join(profile, 'package.json'),
      JSON.stringify({ dsh: { profile: { bundles: [BUNDLE] } } }),
    )
    writeFileSync(join(profile, 'node_modules', BUNDLE, 'package.json'), JSON.stringify({ version: '0.1.0' }))
    expect(needBundle({ DSH_HOME: home }, '0.1.0')).toBe(false)
    expect(needBundle({ DSH_HOME: home }, '0.1.1')).toBe(true)
  })
})

describe('platform', () => {
  it('rejects musl and maps linux-x64 glibc to the native package', () => {
    expect(nativeSpec('linux', 'x64', 'musl').error).toMatch(/musl/)
    expect(nativeSpec('linux', 'x64', 'glibc').name).toBe('@dsh-pager-grok/native-linux-x64-gnu')
    expect(nativeSpec('win32', 'x64', null).bin).toBe('dsh-pager.exe')
    expect(nativeSpec('darwin', 'arm64', null).name).toBe('@dsh-pager-grok/native-darwin-arm64')
  })

  it('accepts node 22.19+ and 24+', () => {
    expect(enginesSatisfied('22.18.0')).toBe(false)
    expect(enginesSatisfied('22.19.0')).toBe(true)
    expect(enginesSatisfied('24.16.0')).toBe(true)
  })
})
