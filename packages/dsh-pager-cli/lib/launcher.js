import { spawn, spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { existsSync, mkdirSync, readFileSync, renameSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { detectLibc, enginesSatisfied, nativeSpec } from './platform.js'

export const PACKAGE = '@dsh-pager-grok/cli'
export const BUNDLE = '@dsh-pager-grok/runtime'
export const PROFILE = 'dsh-pager-grok'

const here = dirname(fileURLToPath(import.meta.url))
const packageRoot = join(here, '..')
const require = createRequire(import.meta.url)

export function readOwnVersion(root = packageRoot) {
  const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  if (pkg.name !== PACKAGE) {
    throw new Error(`package.json name must be ${PACKAGE}`)
  }
  return pkg.version
}

export function dshHome(env = process.env) {
  return env.DSH_HOME && env.DSH_HOME.length > 0 ? env.DSH_HOME : join(homedir(), '.dsh')
}

export function profileDir(env = process.env) {
  return join(dshHome(env), 'profiles', PROFILE)
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

export function needBundle(env = process.env, ownVersion = readOwnVersion()) {
  const manifestPath = join(profileDir(env), 'package.json')
  if (!existsSync(manifestPath)) return true
  let manifest
  try {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
  } catch {
    return true
  }
  const bundles = manifest?.dsh?.profile?.bundles
  if (!Array.isArray(bundles) || !bundles.includes(BUNDLE)) return true
  const installed = join(profileDir(env), 'node_modules', BUNDLE, 'package.json')
  if (!existsSync(installed)) return true
  try {
    const pkg = JSON.parse(readFileSync(installed, 'utf8'))
    return pkg.version !== ownVersion
  } catch {
    return true
  }
}

export function helpText() {
  return `Usage: dsh-pager [command] [pager flags]

Commands:
  doctor       Pre-flight checks (never prints secret values)
  update       Re-align the profile runtime bundle to this CLI version
  uninstall    Remove the profile runtime bundle (keeps $DSH_HOME/sessions)
  repair       Rename a broken profile to a timestamped backup
  version      Print CLI version
  help         Show this help

Pager flags (forwarded to the native binary):
  --hello | --load-only | --list-sessions | --dashboard
  --new | --session <id> | --session-search <query>
  --smoke-interactions | --smoke-queue | --smoke-lifecycle
  --backend <program> | --backend-arg <arg>   (repeatable; values may start with --)

Default product backend (injected unless argv already has --backend or DSH_TUI_SERVER is set):
  --backend <node> --backend-arg <dsh lib/bin.js> --backend-arg --profile --backend-arg ${PROFILE}
`
}

export const LEAF_COMMANDS = new Set(['doctor', 'update', 'uninstall', 'repair'])

export function extraArgsError(command, argv) {
  if (!LEAF_COMMANDS.has(command)) return null
  if (argv.length > 1) return `${command} does not accept extra arguments`
  return null
}

export function resolveDshEntry(env = process.env) {
  if (env.DSH_BIN_JS) {
    if (!existsSync(env.DSH_BIN_JS)) {
      throw new Error(`DSH_BIN_JS does not exist: ${env.DSH_BIN_JS}`)
    }
    return { node: process.execPath, binJs: env.DSH_BIN_JS, custom: true }
  }
  try {
    return { node: process.execPath, binJs: require.resolve('@deepseek-ai/dsh/lib/bin.js'), custom: false }
  } catch (error) {
    throw new Error(
      `cannot resolve @deepseek-ai/dsh/lib/bin.js (${error.message}). Reinstall @dsh-pager-grok/cli.`,
    )
  }
}

export function resolvePagerBinary(opts = {}) {
  if (opts.binPath) return opts.binPath
  const env = opts.env ?? process.env
  if (env.DSH_PAGER_BIN && env.DSH_PAGER_DEV_MODE === '1') {
    return env.DSH_PAGER_BIN
  }
  const spec = nativeSpec(process.platform, process.arch, detectLibc(env))
  if (spec.error) {
    throw new Error(`dsh-pager: ${spec.error}`)
  }
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
    throw new Error(
      `dsh-pager: ${spec.name} did not include ${spec.bin}. Reinstall:\n` +
        `  npm install -g ${PACKAGE} --include=optional`,
    )
  }
  return binary
}

export function productBackendArgs(entry = resolveDshEntry()) {
  return ['--backend', entry.node, '--backend-arg', entry.binJs, '--backend-arg', '--profile', '--backend-arg', PROFILE]
}

export function cliBinDir() {
  return join(packageRoot, 'node_modules', '.bin')
}

export function runDsh(dshArgs, options = {}) {
  const entry = resolveDshEntry(options.env ?? process.env)
  const env = { ...(options.env ?? process.env) }
  env.PATH = `${cliBinDir()}${options.pathSep ?? (process.platform === 'win32' ? ';' : ':')}${env.PATH ?? ''}`
  const result = spawnSync(entry.node, [entry.binJs, ...dshArgs], {
    env,
    stdio: options.stdio ?? 'inherit',
    encoding: 'utf8',
  })
  return result
}

export function ensureProfileBundle(ownVersion, env = process.env) {
  const result = runDsh(['plugin', '--profile', PROFILE, 'add', `${BUNDLE}@${ownVersion}`], { env })
  if (result.status !== 0) {
    const retry = runDsh(
      ['plugin', '--profile', PROFILE, 'add', '-w', `${BUNDLE}@${ownVersion}`],
      { env },
    )
    if (retry.status !== 0) {
      throw new Error(`dsh plugin add ${BUNDLE}@${ownVersion} failed (exit ${retry.status ?? 1})`)
    }
  }
  if (needBundle(env, ownVersion)) {
    throw new Error(
      `profile ${PROFILE} did not install ${BUNDLE}@${ownVersion}. ` +
        `Back up and recreate it with: dsh-pager repair`,
    )
  }
}

export function forwardExit(child) {
  child.on('error', () => {
    process.exit(1)
  })
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
  const binary = resolvePagerBinary(options)
  const child = spawn(binary, argv, { stdio: 'inherit', env })
  forwardExit(child)
  return child
}

export function printDoctor(ownVersion, env = process.env) {
  const lines = [`dsh-pager doctor · ${PACKAGE} ${ownVersion}`]
  let hardFail = false
  const mark = (ok, label, detail) => {
    lines.push(`${ok ? '✓' : '✗'} ${label}${detail ? `  ${detail}` : ''}`)
    return ok
  }
  if (!mark(enginesSatisfied(), 'node', process.versions.node)) hardFail = true
  const spec = nativeSpec(process.platform, process.arch, detectLibc(env))
  if (spec.error) {
    mark(false, 'platform', spec.error)
    hardFail = true
  } else {
    try {
      resolvePagerBinary({ env })
      mark(true, 'native', spec.name)
    } catch (error) {
      mark(false, 'native', error.message.split('\n')[0])
      hardFail = true
    }
  }
  try {
    const entry = resolveDshEntry(env)
    mark(true, 'dsh', entry.custom ? 'DSH_BIN_JS' : '@deepseek-ai/dsh')
  } catch (error) {
    mark(false, 'dsh', error.message.split('\n')[0])
    hardFail = true
  }
  const stdinTty = Boolean(process.stdin.isTTY)
  const stdoutTty = Boolean(process.stdout.isTTY)
  mark(stdinTty && stdoutTty, 'tty', `stdin=${stdinTty ? 'tty' : 'not a tty'} stdout=${stdoutTty ? 'tty' : 'not a tty'}`)
  mark(Boolean(env.DEEPSEEK_API_KEY), 'DEEPSEEK_API_KEY', env.DEEPSEEK_API_KEY ? 'set' : 'not set')
  mark(existsSync(join(dshHome(env), '.credentials.yaml')), '$DSH_HOME/.credentials.yaml', existsSync(join(dshHome(env), '.credentials.yaml')) ? 'present' : 'missing')
  const bundled = !needBundle(env, ownVersion)
  mark(bundled, 'launcher ↔ runtime', bundled ? ownVersion : 'missing or version mismatch')
  try {
    const dump = runDsh(['--profile', PROFILE, '--dump-config'], { env, stdio: 'pipe' })
    if (dump.error) {
      mark(false, 'dump-config', dump.error.message)
      hardFail = true
    } else {
      const text = `${dump.stdout ?? ''}\n${dump.stderr ?? ''}`
      const hasServer = text.includes('@dsh-pager-grok/runtime/server')
      const hasRecovery = text.includes('@dsh-pager-grok/runtime/recovery')
      mark(dump.status === 0 && hasServer && hasRecovery, 'dump-config', dump.status === 0 ? 'runtime rows present' : 'failed')
    }
  } catch (error) {
    mark(false, 'dump-config', error.message)
    hardFail = true
  }
  console.log(lines.join('\n'))
  return hardFail ? 1 : 0
}

export function repairProfile(env = process.env) {
  const dir = profileDir(env)
  if (!existsSync(dir)) {
    console.error(`[dsh-pager] profile ${PROFILE} does not exist`)
    return 0
  }
  const stamp = new Date().toISOString().replaceAll(/[:.]/g, '-')
  const backup = `${dir}.${stamp}.bak`
  mkdirSync(dirname(backup), { recursive: true })
  renameSync(dir, backup)
  console.error(`[dsh-pager] renamed ${dir} -> ${backup}`)
  return 0
}
