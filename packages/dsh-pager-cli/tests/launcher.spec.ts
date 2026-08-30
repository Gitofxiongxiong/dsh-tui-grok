import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  PROFILE_PREFIX,
  UnsupportedDshVersionError,
  commandName,
  extraArgsError,
  hasUserBackend,
  helpText,
  needBundle,
  printDoctor,
  productBackendArgs,
  profileNameFor,
  resolveDshEntry,
  resolveDshSelection,
  resolvePagerBinary,
  supportStatusMessage,
  userBackendKind,
} from '../lib/launcher.js'
import { enginesSatisfied, nativeSpec } from '../lib/platform.js'

const FAMILY_RUNTIME = '@dsh-pager-grok/runtime-apiproxy-v1'

function registry(version = '1.2.3', status = 'supported') {
  return {
    schemaVersion: 1,
    versions: {
      [version]: {
        family: 'apiproxy-v1',
        tag: `dsh-v${version}`,
        commit: 'a'.repeat(40),
        packageManager: 'pnpm@11.7.0',
        runtimePackage: FAMILY_RUNTIME,
        profileSchema: 1,
        status,
        distribution: 'npm',
      },
    },
  }
}

function fakeDsh(version = '1.2.3') {
  const root = mkdtempSync(join(tmpdir(), 'dsh-pager-cli-dsh-'))
  const binJs = join(root, 'lib', 'bin.js')
  mkdirSync(join(root, 'lib'), { recursive: true })
  writeFileSync(join(root, 'package.json'), JSON.stringify({
    name: '@deepseek-ai/dsh',
    version,
  }))
  writeFileSync(binJs, '')
  return { root, binJs }
}

function selection(version = '1.2.3') {
  const dsh = fakeDsh(version)
  return resolveDshSelection({}, {
    entry: { node: process.execPath, binJs: dsh.binJs, source: 'test', custom: true },
    registry: registry(version),
  })
}

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
    expect(resolvePagerBinary({
      env: { ...process.env, DSH_PAGER_BIN: override, DSH_PAGER_DEV_MODE: '1' },
    })).toBe(override)
    let usedOverride = false
    try {
      usedOverride = resolvePagerBinary({ env: { ...process.env, DSH_PAGER_BIN: override } }) === override
    } catch {
      usedOverride = false
    }
    expect(usedOverride).toBe(false)
  })

  it('accepts only doctor --release on leaf commands', () => {
    expect(extraArgsError('doctor', ['doctor', '--release'])).toBeNull()
    expect(extraArgsError('doctor', ['doctor', '--hello'])).toMatch(/does not accept/)
    expect(extraArgsError('update', ['update'])).toBeNull()
    expect(extraArgsError('run', ['--hello'])).toBeNull()
  })

  it('honors DSH_PAGER_LIBC=musl through resolvePagerBinary env', () => {
    if (process.platform !== 'linux') return
    expect(() => resolvePagerBinary({ env: { ...process.env, DSH_PAGER_LIBC: 'musl' } }))
      .toThrow(/musl/)
  })

  it('doctor hard-fails when DSH_BIN_JS is missing', () => {
    const lines: string[] = []
    const orig = console.log
    console.log = (msg) => lines.push(String(msg))
    try {
      const code = printDoctor('0.1.0', {
        ...process.env,
        DSH_BIN_JS: join(tmpdir(), 'missing-dsh.js'),
        DSH_HOME: mkdtempSync(join(tmpdir(), 'dsh-home-')),
      })
      expect(code).toBe(1)
      expect(lines.join('\n')).toMatch(/✗ support/)
    } finally {
      console.log = orig
    }
  })

  it('injects node + bin.js + the selected family profile', () => {
    const selected = selection()
    const args = productBackendArgs({
      ...selected,
      entry: { node: '/usr/bin/node', binJs: '/opt/dsh/lib/bin.js' },
    })
    expect(args).toEqual([
      '--backend', '/usr/bin/node',
      '--backend-arg', '/opt/dsh/lib/bin.js',
      '--backend-arg', '--profile',
      '--backend-arg', `${PROFILE_PREFIX}-apiproxy-v1`,
    ])
  })

  it('documents registry selection and release doctor in launcher help', () => {
    const text = helpText()
    expect(text).toContain('doctor [--release]')
    expect(text).toContain('compat/dsh-support.json')
    expect(text).toContain('--backend-arg')
  })
})

describe('exact DSH resolver', () => {
  it('resolves a supported npm-default entry through the registry', () => {
    const dsh = fakeDsh()
    const resolved = resolveDshSelection({}, {
      resolveDefault: () => dsh.binJs,
      registry: registry(),
    })
    expect(resolved.entry.source).toBe('npm-default')
    expect(resolved.version).toBe('1.2.3')
    expect(resolved.family).toBe('apiproxy-v1')
    expect(resolved.runtimePackage).toBe(FAMILY_RUNTIME)
    expect(resolved.profile).toBe(profileNameFor('apiproxy-v1'))
    expect(resolved.startable).toBe(true)
  })

  it('fails closed for an unlisted exact version with tested versions and a command', () => {
    const dsh = fakeDsh('9.9.9')
    expect(() => resolveDshSelection({}, {
      entry: { node: process.execPath, binJs: dsh.binJs, source: 'test', custom: true },
      registry: registry(),
    })).toThrow(UnsupportedDshVersionError)
    try {
      resolveDshSelection({}, {
        entry: { node: process.execPath, binJs: dsh.binJs, source: 'test', custom: true },
        registry: registry(),
      })
    } catch (error) {
      expect(String((error as Error).message)).toContain('Tested versions: 1.2.3')
      expect(String((error as Error).message)).toContain('npm install -g @deepseek-ai/dsh@1.2.3')
    }
  })

  it('uses an explicit DSH_BIN_JS and reads its owning package version', () => {
    const dsh = fakeDsh()
    const entry = resolveDshEntry({ DSH_BIN_JS: dsh.binJs })
    const resolved = resolveDshSelection({ DSH_BIN_JS: dsh.binJs }, { registry: registry() })
    expect(entry.source).toBe('DSH_BIN_JS')
    expect(resolved.entry.binJs).toBe(dsh.binJs)
    expect(resolved.identity.packageJson).toBe(join(dsh.root, 'package.json'))
  })

  it('has actionable, distinct diagnostics for every public status', () => {
    for (const status of ['supported', 'maintenance', 'candidate', 'experimental', 'unsupported']) {
      expect(supportStatusMessage(status)).toContain(status)
    }
    expect(supportStatusMessage('candidate')).toContain('not yet supported')
    expect(supportStatusMessage('unsupported')).toContain('startup is blocked')
  })
})

describe('family runtime warm skip', () => {
  it('needs a bundle when missing and accepts an aligned selected runtime', () => {
    const selected = selection()
    const missing = { DSH_HOME: join(process.cwd(), '.no-such-dsh-home') }
    expect(needBundle(missing, '0.1.0', selected)).toBe(true)

    const home = mkdtempSync(join(tmpdir(), 'dsh-pager-cli-'))
    const profile = join(home, 'profiles', selected.profile)
    const runtime = join(profile, 'node_modules', ...FAMILY_RUNTIME.split('/'))
    mkdirSync(runtime, { recursive: true })
    writeFileSync(join(profile, 'package.json'), JSON.stringify({
      dsh: { profile: { bundles: [FAMILY_RUNTIME] } },
    }))
    writeFileSync(join(runtime, 'package.json'), JSON.stringify({ version: '0.1.0' }))
    expect(needBundle({ DSH_HOME: home }, '0.1.0', selected)).toBe(false)
    expect(needBundle({ DSH_HOME: home }, '0.1.1', selected)).toBe(true)
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
