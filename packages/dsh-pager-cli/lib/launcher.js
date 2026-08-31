import { spawn, spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { detectLibc, enginesSatisfied, nativeSpec } from './platform.js'
import { runRegistryDependencyGate } from './registry-gate.js'

export const PACKAGE = '@dsh-pager-grok/cli'
export const PROFILE_PREFIX = 'dsh-pager-grok'
export const PROFILE_BASE_BUNDLE = '@deepseek-ai/dsh-base'

const here = dirname(fileURLToPath(import.meta.url))
const packageRoot = join(here, '..')
const require = createRequire(import.meta.url)
const EXACT_VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/
const STARTABLE_STATUSES = new Set(['supported', 'maintenance', 'candidate', 'experimental'])
export const PAGER_SETTING_KEYS = Object.freeze(['theme', 'defaultView', 'reducedMotion'])
export const PROFILE_BUILD_POLICY = `packages:
  - .

nodeLinker: hoisted
autoInstallPeers: false

allowBuilds:
  '@deepseek-ai/dsh-subprocess-local': true
  '@google/genai': false
  koffi: true
  node-pty: true
  protobufjs: false
`
const PROFILE_BUILD_ALLOW = Object.freeze({
  '@deepseek-ai/dsh-subprocess-local': true,
  '@google/genai': false,
  koffi: true,
  'node-pty': true,
  protobufjs: false,
})

export class UnsupportedDshVersionError extends Error {
  constructor(message, evidence = {}) {
    super(message)
    this.name = 'UnsupportedDshVersionError'
    this.evidence = evidence
  }
}

export function readOwnVersion(root = packageRoot) {
  const pkg = readJson(join(root, 'package.json'), 'CLI package manifest')
  if (pkg.name !== PACKAGE) throw new Error(`package.json name must be ${PACKAGE}`)
  return pkg.version
}

export function dshHome(env = process.env) {
  return env.DSH_HOME && env.DSH_HOME.length > 0 ? env.DSH_HOME : join(homedir(), '.dsh')
}

export function profileNameFor(family) {
  if (family !== 'apiproxy-v1' && family !== 'controllers-v2') {
    throw new Error(`unsupported adapter family ${JSON.stringify(family)}`)
  }
  return `${PROFILE_PREFIX}-${family}`
}

export function profileDir(env = process.env, selection) {
  if (selection === undefined) throw new Error('profileDir requires a resolved DSH selection')
  return join(dshHome(env), 'profiles', selection.profile)
}

export function prepareFamilyProfile(ownVersion, selection, env = process.env, options = {}) {
  const target = profileDir(env, selection)
  const legacy = join(dshHome(env), 'profiles', PROFILE_PREFIX)
  const stamp = options.stamp ?? new Date().toISOString().replaceAll(/[:.]/g, '-')
  const log = options.log ?? ((message) => console.error(message))
  let source = existsSync(target) ? target : undefined
  if (source === undefined && existsSync(legacy)) {
    const legacyManifest = readProfileManifest(legacy)
    if (isPagerManagedProfile(legacyManifest)) source = legacy
  }

  if (source === undefined) {
    return { action: 'absent', profile: target, backup: null, migratedSettings: [] }
  }

  const manifest = readProfileManifest(source)
  if (!isPagerManagedProfile(manifest)) {
    throw new Error(
      `profile ${source} is not owned by dsh-pager-grok; choose another DSH_HOME or profile`,
    )
  }
  const metadata = manifest.dshPagerGrok
  const layout = profileLayoutEvidence(source, manifest)
  const aligned = source === target
    && metadata?.managed === true
    && metadata.adapterFamily === selection.family
    && metadata.profileSchema === selection.profileSchema
    && layout.ok
  const pagerSettings = copyPagerSettings(metadata?.pagerSettings)
  if (aligned) {
    writeOwnedProfileManifest(target, manifest, ownVersion, selection, pagerSettings)
    ensureProfileBuildPolicy(selection, env)
    return {
      action: 'aligned',
      profile: target,
      backup: null,
      migratedSettings: Object.keys(pagerSettings),
    }
  }

  const backup = uniqueBackupPath(source, stamp)
  renameSync(source, backup)
  writeOwnedProfileManifest(target, {}, ownVersion, selection, pagerSettings)
  ensureProfileBuildPolicy(selection, env)
  const migrated = Object.keys(pagerSettings)
  log(`[dsh-pager] family profile migrated: ${source} -> ${target}`)
  log(`[dsh-pager] previous profile backup: ${backup}`)
  log(`[dsh-pager] migrated pager settings: ${migrated.length > 0 ? migrated.join(', ') : 'none'}`)
  log(`[dsh-pager] projection cache was not migrated; it remains only in ${backup}`)
  log('[dsh-pager] $DSH_HOME/sessions and credentials were not read or modified')
  return { action: 'migrated', profile: target, backup, migratedSettings: migrated }
}

export function writeProfileOwnership(ownVersion, selection, env = process.env) {
  const target = profileDir(env, selection)
  if (!existsSync(target)) throw new Error(`profile does not exist: ${target}`)
  const manifest = readProfileManifest(target)
  if (!isPagerManagedProfile(manifest)) {
    throw new Error(`profile ${target} is not owned by dsh-pager-grok`)
  }
  const pagerSettings = copyPagerSettings(manifest.dshPagerGrok?.pagerSettings)
  writeOwnedProfileManifest(target, manifest, ownVersion, selection, pagerSettings)
}

export function isPagerManagedProfile(manifest) {
  if (!isRecord(manifest)) return false
  if (manifest.dshPagerGrok?.managed === true) return true
  const bundles = Array.isArray(manifest?.dsh?.profile?.bundles)
    ? manifest.dsh.profile.bundles
    : []
  const dependencies = isRecord(manifest.dependencies) ? Object.keys(manifest.dependencies) : []
  return [...bundles, ...dependencies]
    .some(name => typeof name === 'string' && name.startsWith('@dsh-pager-grok/'))
}

export function userBackendKind(argv, env = process.env) {
  if (argv.includes('--backend')) return 'argv'
  const fromEnv = env.DSH_TUI_SERVER
  if (fromEnv === undefined) return 'none'
  if (typeof fromEnv !== 'string' || fromEnv.trim().length === 0) return 'blank'
  return 'env'
}

export function hasUserBackend(argv, env = process.env) {
  const kind = userBackendKind(argv, env)
  return kind === 'argv' || kind === 'env'
}

export function commandName(argv) {
  const first = argv[0]
  if (first === undefined) return 'run'
  if (first === 'help' || first === '--help' || first === '-h') return 'help'
  if (first === 'version' || first === '--version' || first === '-v') return 'version'
  if (first === 'doctor' || first === 'update' || first === 'uninstall' || first === 'repair') {
    return first
  }
  return 'run'
}

export function supportRegistryPath(env = process.env, root = packageRoot) {
  if (env.DSH_PAGER_SUPPORT_REGISTRY) {
    if (env.DSH_PAGER_DEV_MODE !== '1') {
      throw new Error('DSH_PAGER_SUPPORT_REGISTRY requires DSH_PAGER_DEV_MODE=1')
    }
    return resolve(env.DSH_PAGER_SUPPORT_REGISTRY)
  }
  const candidates = [
    resolve(root, '..', '..', 'compat', 'dsh-support.json'),
    join(root, 'lib', 'dsh-support.json'),
  ]
  const found = candidates.find(candidate => existsSync(candidate))
  if (found !== undefined) return found
  throw new Error(`support registry is missing; checked: ${candidates.join(', ')}`)
}

export function readSupportRegistry(path = supportRegistryPath()) {
  const registry = readJson(path, 'DSH support registry')
  if (registry?.schemaVersion !== 1 || !isRecord(registry.versions)) {
    throw new Error(`invalid DSH support registry: ${path}`)
  }
  for (const [version, entry] of Object.entries(registry.versions)) {
    if (!EXACT_VERSION.test(version) || !isRecord(entry)) {
      throw new Error(`invalid DSH support entry for ${JSON.stringify(version)}`)
    }
    for (const key of ['family', 'runtimePackage', 'profileSchema', 'status', 'distribution']) {
      if (!Object.hasOwn(entry, key)) throw new Error(`DSH support ${version} is missing ${key}`)
    }
  }
  return registry
}

export function resolveDshEntry(env = process.env, options = {}) {
  if (env.DSH_BIN_JS) {
    if (!existsSync(env.DSH_BIN_JS)) throw new Error(`DSH_BIN_JS does not exist: ${env.DSH_BIN_JS}`)
    return { node: process.execPath, binJs: resolve(env.DSH_BIN_JS), source: 'DSH_BIN_JS', custom: true }
  }
  try {
    const resolveDefault = options.resolveDefault ?? (() => require.resolve('@deepseek-ai/dsh/lib/bin.js'))
    return { node: process.execPath, binJs: resolveDefault(), source: 'npm-default', custom: false }
  } catch (error) {
    throw new Error(
      `cannot resolve @deepseek-ai/dsh/lib/bin.js (${error.message}). Reinstall ${PACKAGE}.`,
    )
  }
}

export function readDshIdentity(entry) {
  let cursor = dirname(resolve(entry.binJs))
  for (;;) {
    const manifestPath = join(cursor, 'package.json')
    if (existsSync(manifestPath)) {
      const manifest = readJson(manifestPath, 'DSH package manifest')
      if (manifest.name === '@deepseek-ai/dsh') {
        if (typeof manifest.version !== 'string' || !EXACT_VERSION.test(manifest.version)) {
          throw new Error(`DSH package has non-exact version ${JSON.stringify(manifest.version)}: ${manifestPath}`)
        }
        return { version: manifest.version, packageJson: manifestPath, packageRoot: cursor }
      }
    }
    const parent = dirname(cursor)
    if (parent === cursor) break
    cursor = parent
  }
  throw new Error(`cannot locate @deepseek-ai/dsh package.json above ${entry.binJs}`)
}

export function testedVersionSummary(registry) {
  return Object.entries(registry.versions)
    .map(([version, entry]) => `${version} (${entry.status}, ${entry.distribution}, ${entry.family})`)
    .join(', ')
}

export function recommendedInstall(registry) {
  const order = ['supported', 'candidate', 'maintenance', 'experimental']
  const row = order.flatMap(status => Object.entries(registry.versions)
    .filter(([, entry]) => entry.status === status))[0]
  if (row === undefined) return `npm install -g ${PACKAGE}`
  return `npm install -g @deepseek-ai/dsh@${row[0]} ${PACKAGE}`
}

export function resolveDshSelection(env = process.env, options = {}) {
  const entry = options.entry ?? resolveDshEntry(env, options)
  const identity = options.identity ?? readDshIdentity(entry)
  const registryPath = options.registry === undefined
    ? supportRegistryPath(env, options.packageRoot ?? packageRoot)
    : options.registryPath ?? '<in-memory>'
  const registry = options.registry ?? readSupportRegistry(registryPath)
  const support = registry.versions[identity.version]
  if (support === undefined) {
    throw new UnsupportedDshVersionError(
      `unsupported DSH version ${identity.version} from ${identity.packageJson}. ` +
        `Tested versions: ${testedVersionSummary(registry)}. Recommended: ${recommendedInstall(registry)}`,
      { entry, identity, registry, registryPath },
    )
  }
  return {
    entry,
    identity,
    registry,
    registryPath,
    version: identity.version,
    family: support.family,
    runtimePackage: support.runtimePackage,
    profileSchema: support.profileSchema,
    status: support.status,
    distribution: support.distribution,
    profile: profileNameFor(support.family),
    startable: STARTABLE_STATUSES.has(support.status),
  }
}

export function assertStartableSelection(selection) {
  if (selection.startable) return selection
  throw new UnsupportedDshVersionError(
    `DSH ${selection.version} is listed as ${selection.status} and cannot start. ` +
      `Tested versions: ${testedVersionSummary(selection.registry)}. ` +
      `Recommended: ${recommendedInstall(selection.registry)}`,
    { entry: selection.entry, identity: selection.identity, registry: selection.registry },
  )
}

export function needBundle(env = process.env, ownVersion = readOwnVersion(), selection) {
  const dir = profileDir(env, selection)
  const manifestPath = join(dir, 'package.json')
  if (!existsSync(manifestPath)) return true
  let manifest
  try {
    manifest = readJson(manifestPath, 'DSH profile manifest')
  } catch {
    return true
  }
  const bundles = manifest?.dsh?.profile?.bundles
  if (!Array.isArray(bundles) || !bundles.includes(selection.runtimePackage)) return true
  const installed = join(dir, 'node_modules', ...selection.runtimePackage.split('/'), 'package.json')
  if (!existsSync(installed)) return true
  try {
    const pkg = readJson(installed, 'profile runtime manifest')
    return pkg.version !== ownVersion
  } catch {
    return true
  }
}

export function helpText() {
  return `Usage: dsh-pager [command] [pager flags]

Commands:
  doctor [--release]  Version/profile checks; --release verifies registry dependencies
  update              Re-align the selected family runtime to this CLI version
  uninstall           Remove the selected family runtime (keeps $DSH_HOME/sessions)
  repair              Rename the selected family profile to a timestamped backup
  version             Print CLI version
  help                Show this help

Pager flags (forwarded to the native binary):
  --hello | --load-only | --list-sessions | --dashboard
  --resume [id] | --continue | --new | --session <id> | --session-search <query>
  --smoke-interactions | --smoke-queue | --smoke-lifecycle
  --backend <program> | --backend-arg <arg>   (repeatable; values may start with --)

Session startup:
  No session flag starts a new conversation. Use --resume/-r (or /resume in the TUI)
  to open history; --new, --session and --session-search remain compatibility flags.

Default product backend (unless argv has --backend or DSH_TUI_SERVER is set):
  exact DSH package version -> compat/dsh-support.json -> family runtime/profile -> native pager
`
}

export const LEAF_COMMANDS = new Set(['doctor', 'update', 'uninstall', 'repair'])

export function extraArgsError(command, argv) {
  if (!LEAF_COMMANDS.has(command)) return null
  if (command === 'doctor' && argv.length === 2 && argv[1] === '--release') return null
  if (argv.length > 1) return `${command} does not accept ${argv.slice(1).join(' ')}`
  return null
}

export function resolvePagerBinary(opts = {}) {
  if (opts.binPath) return opts.binPath
  const env = opts.env ?? process.env
  if (env.DSH_PAGER_BIN && env.DSH_PAGER_DEV_MODE === '1') return env.DSH_PAGER_BIN
  const spec = nativeSpec(process.platform, process.arch, detectLibc(env))
  if (spec.error) throw new Error(`dsh-pager: ${spec.error}`)
  let packageJson
  try {
    packageJson = require.resolve(`${spec.name}/package.json`)
  } catch {
    throw new Error(
      `dsh-pager: native package ${spec.name} is missing. Reinstall without omitting optional deps:\n` +
        `  npm install -g ${PACKAGE} --include=optional`,
    )
  }
  const binary = join(dirname(packageJson), 'bin', spec.bin)
  if (!existsSync(binary)) {
    throw new Error(`dsh-pager: ${spec.name} did not include ${spec.bin}. Reinstall ${PACKAGE}.`)
  }
  return binary
}

export function productBackendArgs(selection) {
  return [
    '--backend', selection.entry.node,
    '--backend-arg', selection.entry.binJs,
    '--backend-arg', '--profile',
    '--backend-arg', selection.profile,
  ]
}

export function cliBinDir() {
  return join(packageRoot, 'node_modules', '.bin')
}

export function runDsh(dshArgs, options = {}) {
  const entry = options.entry ?? resolveDshEntry(options.env ?? process.env)
  const env = { ...(options.env ?? process.env) }
  env.PATH = `${cliBinDir()}${options.pathSep ?? (process.platform === 'win32' ? ';' : ':')}${env.PATH ?? ''}`
  return spawnSync(entry.node, [entry.binJs, ...dshArgs], {
    env,
    stdio: options.stdio ?? 'inherit',
    encoding: 'utf8',
  })
}

export function ensureProfileBundle(ownVersion, selection, env = process.env) {
  if (selection.distribution === 'source-only') {
    throw new Error(
      `${selection.runtimePackage} is source-only. Prepare ${selection.profile} with the development ` +
        `setup for DSH ${selection.version}; automatic registry installation is disabled.`,
    )
  }
  const runtimeOverride = env.DSH_PAGER_RUNTIME_SPEC
  if (runtimeOverride && env.DSH_PAGER_DEV_MODE !== '1') {
    throw new Error('DSH_PAGER_RUNTIME_SPEC requires DSH_PAGER_DEV_MODE=1')
  }
  if (!existsSync(join(profileDir(env, selection), 'package.json'))) {
    const initialized = runDsh(['plugin', '--profile', selection.profile, 'list'], {
      env,
      entry: selection.entry,
    })
    if (initialized.status !== 0) {
      throw new Error(`dsh failed to initialize profile ${selection.profile}`)
    }
  }
  ensureProfileBuildPolicy(selection, env)
  const spec = runtimeOverride || `${selection.runtimePackage}@${ownVersion}`
  const args = ['plugin', '--profile', selection.profile, 'add', spec]
  let result = runDsh(args, { env, entry: selection.entry })
  if (result.status !== 0) {
    result = runDsh(['plugin', '--profile', selection.profile, 'add', '-w', spec], {
      env,
      entry: selection.entry,
    })
  }
  if (result.status !== 0) {
    throw new Error(
      `dsh plugin add ${spec} failed (exit ${result.status ?? 1}). ` +
        `Install ${spec}, or configure a family runtime explicitly for development.`,
    )
  }
  if (needBundle(env, ownVersion, selection)) {
    throw new Error(`profile ${selection.profile} did not install ${spec}; run dsh-pager repair`)
  }
  writeProfileOwnership(ownVersion, selection, env)
}

export function ensureProfileBuildPolicy(selection, env = process.env) {
  const path = join(profileDir(env, selection), 'pnpm-workspace.yaml')
  mkdirSync(dirname(path), { recursive: true })
  if (!existsSync(path)) {
    writeFileSync(path, PROFILE_BUILD_POLICY, { flag: 'wx' })
    return { path, created: true, updated: false }
  }
  const original = readFileSync(path, 'utf8')
  let next = original
  if (!/^allowBuilds:\s*$/m.test(next)) {
    next = `${next.trimEnd()}\n${PROFILE_BUILD_POLICY.slice(PROFILE_BUILD_POLICY.indexOf('allowBuilds:'))}`
  } else {
    const missing = []
    for (const [name, allowed] of Object.entries(PROFILE_BUILD_ALLOW)) {
      const escaped = name.replaceAll(/[.*+?^${}()|[\]\\]/g, '\\$&')
      const generated = new RegExp(`^(\\s+(?:'${escaped}'|${escaped}):)\\s+set this to true or false\\s*$`, 'm')
      next = next.replace(generated, `$1 ${String(allowed)}`)
      const present = new RegExp(`^\\s+(?:'${escaped}'|${escaped}):\\s+.+$`, 'm')
      if (!present.test(next)) {
        const key = name.startsWith('@') ? `'${name}'` : name
        missing.push(`  ${key}: ${String(allowed)}`)
      }
    }
    if (missing.length > 0) {
      next = next.replace(/^allowBuilds:\s*$/m, `allowBuilds:\n${missing.join('\n')}`)
    }
  }
  next = setTopLevelPolicyValue(next, 'nodeLinker', 'hoisted')
  next = setTopLevelPolicyValue(next, 'autoInstallPeers', 'false')
  if (next === original) return { path, created: false, updated: false }
  writeFileSync(path, next)
  return { path, created: false, updated: true }
}

export function forwardExit(child) {
  child.on('error', () => process.exit(1))
  child.on('exit', (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal)
      return
    }
    process.exit(code ?? 0)
  })
}

export function spawnPager(argv, options = {}) {
  const env = { ...(options.env ?? process.env), DSH_PAGER_ROLE: 'launcher' }
  if (options.selection !== undefined) {
    env.DSH_PAGER_EXPECTED_ADAPTER_FAMILY = options.selection.family
    env.DSH_PAGER_EXPECTED_DSH_VERSION = options.selection.version
    env.DSH_PAGER_EXPECTED_PROFILE_SCHEMA = String(options.selection.profileSchema)
  }
  const binary = resolvePagerBinary(options)
  const child = spawn(binary, argv, { stdio: 'inherit', env })
  forwardExit(child)
  return child
}

export function supportStatusMessage(status) {
  const messages = {
    supported: 'supported: full release and E2E gates',
    maintenance: 'maintenance: compatibility fixes and matrix coverage',
    candidate: 'candidate: pre-release evidence only; not yet supported',
    experimental: 'experimental: source-only and not an npm default',
    unsupported: 'unsupported: startup is blocked; install a tested version',
  }
  return messages[status] ?? `unknown status: ${status}`
}

export function printDoctor(ownVersion, env = process.env, options = {}) {
  const lines = [`dsh-pager doctor · ${PACKAGE} ${ownVersion}`]
  let hardFail = false
  const mark = (ok, label, detail) => {
    lines.push(`${ok ? '✓' : '✗'} ${label}${detail ? `  ${detail}` : ''}`)
    return ok
  }
  if (!mark(enginesSatisfied(), 'node', process.versions.node)) hardFail = true
  const spec = nativeSpec(process.platform, process.arch, detectLibc(env))
  if (spec.error) {
    mark(false, 'native', spec.error)
    hardFail = true
  } else {
    try {
      const binary = resolvePagerBinary({ env })
      const manifestPath = require.resolve(`${spec.name}/package.json`)
      const nativeVersion = readJson(manifestPath, 'native package manifest').version
      const aligned = nativeVersion === ownVersion
      mark(aligned, 'pager CLI ↔ native', `${spec.name}@${nativeVersion} binary=${binary}`)
      if (!aligned) hardFail = true
    } catch (error) {
      mark(false, 'pager CLI ↔ native', firstLine(error))
      hardFail = true
    }
  }

  try {
    const selection = resolveDshSelection(env, options)
    mark(true, 'DSH entry', `${selection.entry.source} ${selection.entry.binJs}`)
    mark(true, 'DSH package', `${selection.version} ${selection.identity.packageJson}`)
    mark(selection.startable, 'support',
      `${selection.status}/${selection.distribution} · ${supportStatusMessage(selection.status)}`)
    if (!selection.startable) hardFail = true
    mark(true, 'adapter', `${selection.family} runtime=${selection.runtimePackage}`)
    const profile = profileDoctorEvidence(selection, ownVersion, env)
    mark(profile.ok, 'profile', profile.detail)
    if (!profile.ok) hardFail = true
    const runtime = runtimeDoctorEvidence(selection, ownVersion, env)
    mark(runtime.ok, 'pager CLI ↔ runtime', runtime.detail)
    if (!runtime.ok) hardFail = true
    mark(runtime.capabilities.length > 0, 'capabilities', runtime.capabilities.length > 0
      ? runtime.capabilities.join(', ')
      : `unavailable until ${selection.runtimePackage} is installed and asserts its adapter`)
    if (runtime.capabilities.length === 0) hardFail = true
  } catch (error) {
    const evidence = error instanceof UnsupportedDshVersionError ? error.evidence : {}
    if (evidence.entry !== undefined) mark(true, 'DSH entry', `${evidence.entry.source} ${evidence.entry.binJs}`)
    if (evidence.identity !== undefined) mark(true, 'DSH package', `${evidence.identity.version} ${evidence.identity.packageJson}`)
    mark(false, 'support', firstLine(error))
    hardFail = true
  }

  const stdinTty = Boolean(process.stdin.isTTY)
  const stdoutTty = Boolean(process.stdout.isTTY)
  mark(stdinTty && stdoutTty, 'tty', `stdin=${stdinTty ? 'tty' : 'not a tty'} stdout=${stdoutTty ? 'tty' : 'not a tty'}`)
  mark(Boolean(env.DEEPSEEK_API_KEY), 'DEEPSEEK_API_KEY', env.DEEPSEEK_API_KEY ? 'set' : 'not set')
  mark(existsSync(join(dshHome(env), '.credentials.yaml')), '$DSH_HOME/.credentials.yaml',
    existsSync(join(dshHome(env), '.credentials.yaml')) ? 'present' : 'missing')
  if (options.release) {
    try {
      const selection = resolveDshSelection(env, options)
      const runtimeManifest = findRuntimeManifest(selection, env)
      if (runtimeManifest === undefined) {
        throw new Error(`missing ${selection.runtimePackage}; cannot inspect release dependencies`)
      }
      const gate = runRegistryDependencyGate([join(packageRoot, 'package.json'), runtimeManifest], {
        runner: options.registryRunner,
        env,
      })
      mark(gate.ok, 'release registry dependencies', gate.ok
        ? `${gate.checks.length} exact non-optional dependencies available`
        : gate.failures.join('; '))
      if (!gate.ok) hardFail = true
    } catch (error) {
      mark(false, 'release registry dependencies', firstLine(error))
      hardFail = true
    }
  } else {
    lines.push('- release registry dependencies  run doctor --release')
  }
  console.log(lines.join('\n'))
  return hardFail ? 1 : 0
}

function profileDoctorEvidence(selection, ownVersion, env) {
  const dir = profileDir(env, selection)
  if (!existsSync(join(dir, 'package.json'))) {
    return {
      ok: false,
      detail: `missing ${dir}; expected family=${selection.family} schema=${selection.profileSchema}`,
    }
  }
  try {
    const manifest = readProfileManifest(dir)
    const metadata = manifest.dshPagerGrok
    const layout = profileLayoutEvidence(dir, manifest)
    const ok = metadata?.managed === true
      && metadata.adapterFamily === selection.family
      && metadata.dshVersion === selection.version
      && metadata.profileSchema === selection.profileSchema
      && metadata.runtimeVersion === ownVersion
      && layout.ok
    return {
      ok,
      detail: `${dir} family=${metadata?.adapterFamily ?? '?'} dsh=${metadata?.dshVersion ?? '?'} schema=${metadata?.profileSchema ?? '?'} runtime=${metadata?.runtimeVersion ?? '?'} layout=${layout.ok ? 'ready' : layout.reason}`,
    }
  } catch (error) {
    return { ok: false, detail: firstLine(error) }
  }
}

function runtimeDoctorEvidence(selection, ownVersion, env) {
  const manifestPath = findRuntimeManifest(selection, env)
  if (manifestPath === undefined) {
    return {
      ok: false,
      detail: `missing ${selection.runtimePackage}; install it or set DSH_PAGER_RUNTIME_ROOT in development`,
      capabilities: [],
    }
  }
  try {
    const manifest = readJson(manifestPath, 'runtime package manifest')
    const metadata = manifest.dshPagerGrok
    const capabilities = isRecord(metadata?.capabilities)
      ? Object.entries(metadata.capabilities).map(([name, enabled]) => `${name}=${String(enabled)}`)
      : []
    const aligned = manifest.name === selection.runtimePackage && manifest.version === ownVersion
    const familyAligned = metadata?.adapterFamily === selection.family && metadata?.profileSchema === selection.profileSchema
    return {
      ok: aligned && familyAligned,
      detail: `${manifest.name ?? '?'}@${manifest.version ?? '?'} family=${metadata?.adapterFamily ?? '?'} schema=${metadata?.profileSchema ?? '?'} path=${manifestPath}`,
      capabilities,
    }
  } catch (error) {
    return { ok: false, detail: firstLine(error), capabilities: [] }
  }
}

function findRuntimeManifest(selection, env) {
  const candidates = []
  if (env.DSH_PAGER_RUNTIME_ROOT) candidates.push(join(resolve(env.DSH_PAGER_RUNTIME_ROOT), 'package.json'))
  candidates.push(join(profileDir(env, selection), 'node_modules', ...selection.runtimePackage.split('/'), 'package.json'))
  return candidates.find(candidate => existsSync(candidate))
}

export function repairProfile(selection, env = process.env) {
  const dir = profileDir(env, selection)
  if (!existsSync(dir)) {
    console.error(`[dsh-pager] profile ${selection.profile} does not exist`)
    return 0
  }
  const manifest = readProfileManifest(dir)
  if (!isPagerManagedProfile(manifest)) {
    throw new Error(`profile ${dir} is not owned by dsh-pager-grok; refusing to rename it`)
  }
  const stamp = new Date().toISOString().replaceAll(/[:.]/g, '-')
  const backup = `${dir}.${stamp}.bak`
  mkdirSync(dirname(backup), { recursive: true })
  renameSync(dir, backup)
  console.error(`[dsh-pager] renamed ${dir} -> ${backup}`)
  return 0
}

function copyPagerSettings(value) {
  if (!isRecord(value)) return {}
  const copied = {}
  if (typeof value.theme === 'string') copied.theme = value.theme
  if (typeof value.defaultView === 'string') copied.defaultView = value.defaultView
  if (typeof value.reducedMotion === 'boolean') copied.reducedMotion = value.reducedMotion
  return copied
}

function writeOwnedProfileManifest(dir, existing, ownVersion, selection, pagerSettings) {
  mkdirSync(dir, { recursive: true })
  const existingDsh = isRecord(existing.dsh) ? existing.dsh : {}
  const existingProfile = isRecord(existingDsh.profile) ? existingDsh.profile : {}
  const existingBundles = Array.isArray(existingProfile.bundles)
    ? existingProfile.bundles.filter(bundle => typeof bundle === 'string')
    : []
  const bundles = [...new Set([PROFILE_BASE_BUNDLE, ...existingBundles])]
  const manifest = {
    name: `dsh-pager-profile-${selection.family}`,
    private: true,
    ...existing,
    dsh: {
      ...existingDsh,
      profile: {
        ...existingProfile,
        bundles,
      },
    },
    dshPagerGrok: {
      managed: true,
      adapterFamily: selection.family,
      dshVersion: selection.version,
      profileSchema: selection.profileSchema,
      runtimeVersion: ownVersion,
      ...(Object.keys(pagerSettings).length > 0 ? { pagerSettings } : {}),
    },
  }
  const manifestPath = join(dir, 'package.json')
  const temporary = `${manifestPath}.dsh-pager.tmp`
  writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 })
  renameSync(temporary, manifestPath)
}

function profileLayoutEvidence(dir, manifest) {
  const bundles = Array.isArray(manifest?.dsh?.profile?.bundles)
    ? manifest.dsh.profile.bundles
    : []
  if (!bundles.includes(PROFILE_BASE_BUNDLE)) {
    return { ok: false, reason: `missing ${PROFILE_BASE_BUNDLE}` }
  }
  const policyPath = join(dir, 'pnpm-workspace.yaml')
  if (!existsSync(policyPath)) return { ok: false, reason: 'missing pnpm-workspace.yaml' }
  const policy = readFileSync(policyPath, 'utf8')
  if (!profileBuildPolicyAligned(policy)) {
    return { ok: false, reason: 'pnpm policy mismatch' }
  }
  return { ok: true, reason: 'ready' }
}

function profileBuildPolicyAligned(policy) {
  if (!/^nodeLinker:\s*hoisted\s*$/m.test(policy)) return false
  if (!/^autoInstallPeers:\s*false\s*$/m.test(policy)) return false
  for (const [name, allowed] of Object.entries(PROFILE_BUILD_ALLOW)) {
    const escaped = name.replaceAll(/[.*+?^${}()|[\]\\]/g, '\\$&')
    const key = `(?:'${escaped}'|${escaped})`
    if (!new RegExp(`^\\s+${key}:\\s+${String(allowed)}\\s*$`, 'm').test(policy)) return false
  }
  return true
}

function setTopLevelPolicyValue(policy, key, value) {
  const line = `${key}: ${value}`
  const existing = new RegExp(`^${key}:.*$`, 'm')
  if (existing.test(policy)) return policy.replace(existing, line)
  if (/^allowBuilds:\s*$/m.test(policy)) {
    return policy.replace(/^allowBuilds:\s*$/m, `${line}\n\nallowBuilds:`)
  }
  return `${policy.trimEnd()}\n${line}\n`
}

function uniqueBackupPath(source, stamp) {
  const base = `${source}.backup-${stamp}`
  if (!existsSync(base)) return base
  for (let suffix = 2; suffix < 10_000; suffix += 1) {
    const candidate = `${base}-${suffix}`
    if (!existsSync(candidate)) return candidate
  }
  throw new Error(`cannot allocate profile backup path for ${source}`)
}

function readProfileManifest(dir) {
  const manifestPath = join(dir, 'package.json')
  if (!existsSync(manifestPath)) throw new Error(`profile manifest is missing: ${manifestPath}`)
  return readJson(manifestPath, 'DSH profile manifest')
}

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function readJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'))
  } catch (error) {
    throw new Error(`${label} is not valid JSON at ${path}: ${error.message}`)
  }
}

function firstLine(error) {
  return String(error?.message ?? error).split('\n')[0]
}
