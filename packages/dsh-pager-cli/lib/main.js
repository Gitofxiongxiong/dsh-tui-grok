import { PACKAGE, PROFILE, BUNDLE, commandName, extraArgsError, helpText, needBundle, printDoctor, productBackendArgs, readOwnVersion, repairProfile, resolveDshEntry, runDsh, spawnPager, ensureProfileBundle, userBackendKind } from './launcher.js'
import { enginesSatisfied, nativeSpec } from './platform.js'

function refuseNested() {
  const role = process.env.DSH_PAGER_ROLE
  if (role === 'pager' || role === 'launcher') {
    console.error('dsh-pager: refusing nested dsh-pager')
    process.exit(1)
  }
}

function requireInteractiveTerminal(command, argv) {
  if (command !== 'run') return
  const flags = new Set(['--hello', '--load-only', '--list-sessions', '--dashboard', '--smoke-interactions', '--smoke-queue', '--smoke-lifecycle'])
  if (argv.some((token) => flags.has(token))) return
  if (process.stdin.isTTY && process.stdout.isTTY) return
  console.error('dsh-pager requires an interactive terminal')
  process.exit(2)
}

function run() {
  refuseNested()
  const argv = process.argv.slice(2)
  const command = commandName(argv)
  const ownVersion = readOwnVersion()
  const extra = extraArgsError(command, argv)
  if (extra) {
    console.error(`dsh-pager: ${extra}`)
    process.exit(2)
  }

  if (command === 'help') {
    process.stdout.write(helpText())
    return
  }
  if (command === 'version') {
    console.log(`${PACKAGE} ${ownVersion}`)
    return
  }
  if (command === 'doctor') {
    process.exit(printDoctor(ownVersion))
  }
  if (command === 'repair') {
    process.exit(repairProfile())
  }
  if (command === 'uninstall') {
    const result = runDsh(['plugin', '--profile', PROFILE, 'remove', BUNDLE])
    process.exit(result.status ?? 1)
  }
  if (command === 'update') {
    try {
      ensureProfileBundle(ownVersion)
    } catch (error) {
      console.error(`dsh-pager: ${error.message}`)
      process.exit(1)
    }
    console.error(`[dsh-pager] runtime aligned to ${ownVersion}. To upgrade the CLI itself:\n  npm install -g ${PACKAGE}@${ownVersion}`)
    return
  }

  if (!enginesSatisfied()) {
    console.error(`dsh-pager: node ${process.versions.node} does not satisfy ^22.19.0 || >=24.0.0`)
    process.exit(1)
  }
  const spec = nativeSpec()
  if (spec.error) {
    console.error(`dsh-pager: ${spec.error}`)
    process.exit(2)
  }
  requireInteractiveTerminal(command, argv)

  const backendKind = userBackendKind(argv)
  if (backendKind === 'blank') {
    console.error('dsh-pager: DSH_TUI_SERVER is empty')
    process.exit(2)
  }
  if (backendKind !== 'none') {
    spawnPager(argv)
    return
  }
  if (needBundle(process.env, ownVersion)) {
    ensureProfileBundle(ownVersion)
  }
  const backend = productBackendArgs(resolveDshEntry())
  spawnPager([...argv, ...backend])
}

run()
