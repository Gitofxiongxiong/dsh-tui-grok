#!/usr/bin/env node
/**
 * Copy built protocol/server libs into one family runtime and rewrite
 * in-workspace grok specifiers to relative paths. The apiproxy-v1 publish
 * candidate deliberately excludes controllers-v2 files and alpha imports.
 */
import { spawnSync } from 'node:child_process'
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { copyPackageLicenses } from './copy-package-licenses.mjs'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const variant = process.argv[2] ?? 'controllers-v2'
const variants = {
  'controllers-v2': {
    runtimeDir: 'dsh-pager-runtime',
    patch: 'packages/dsh-tui-embedded/cordis.patch.yml',
    patchServer: '@dsh-pager-grok/tui-server',
    runtimeServer: '@dsh-pager-grok/runtime/server',
  },
  'apiproxy-v1': {
    runtimeDir: 'dsh-pager-runtime-apiproxy-v1',
    patch: 'compat/fixtures/dsh-0.1.1-rc.2/cordis.patch.yml',
    patchServer: '@dsh-pager-grok/compat-dsh-0.1.1-rc.2',
    runtimeServer: '@dsh-pager-grok/runtime-apiproxy-v1/server',
  },
}
const config = variants[variant]
if (config === undefined) {
  console.error(`assemble-runtime: unsupported variant ${JSON.stringify(variant)}`)
  process.exit(2)
}
const runtimeRoot = join(repoRoot, 'packages', config.runtimeDir)
const outLib = join(runtimeRoot, 'lib')

const PACKAGES = [
  { name: '@dsh-pager-grok/tui-protocol', dir: 'dsh-tui-protocol', dest: 'protocol' },
  { name: '@dsh-pager-grok/tui-server', dir: 'dsh-tui-server', dest: 'server' },
]

const SKIP_NAMES = new Set(['tsconfig.tsbuildinfo'])

function fail(message) {
  console.error(`assemble-runtime: ${message}`)
  process.exit(1)
}

function run(command, args) {
  const result = spawnSync(command, args, { cwd: repoRoot, stdio: 'inherit' })
  if (result.status !== 0) {
    fail(`${command} ${args.join(' ')} failed`)
  }
}

function walkFiles(root, suffix) {
  const out = []
  const stack = [root]
  while (stack.length > 0) {
    const current = stack.pop()
    for (const entry of readdirSync(current)) {
      if (SKIP_NAMES.has(entry)) continue
      const full = join(current, entry)
      const st = statSync(full)
      if (st.isDirectory()) {
        stack.push(full)
      } else if (entry.endsWith(suffix) && !entry.endsWith('.map')) {
        out.push(full)
      }
    }
  }
  return out
}

function rewriteSpecifiers(source, filePath) {
  return source.replaceAll(
    /(['"])(@dsh-pager-grok\/tui-(?:protocol|server))(\/[^'"]+)?\1/g,
    (match, quote, pkg, subpath) => {
      const dest = PACKAGES.find((item) => item.name === pkg)?.dest
      if (!dest) return match
      const targetFile = subpath ? `${dest}${subpath}.js` : `${dest}/index.js`
      const fromDir = dirname(filePath)
      const absoluteTarget = join(outLib, targetFile.replace(/\.js\.js$/, '.js'))
      let rel = relative(fromDir, absoluteTarget).replaceAll('\\', '/')
      if (!rel.startsWith('.')) rel = `./${rel}`
      return `${quote}${rel}${quote}`
    },
  )
}

function copyTree(src, dest) {
  mkdirSync(dest, { recursive: true })
  for (const entry of readdirSync(src)) {
    if (SKIP_NAMES.has(entry) || entry.endsWith('.map')) continue
    const from = join(src, entry)
    const to = join(dest, entry)
    const st = statSync(from)
    if (st.isDirectory()) {
      copyTree(from, to)
    } else if (entry.endsWith('.js') || entry.endsWith('.d.ts')) {
      cpSync(from, to)
    }
  }
}

function copyServerTree(src, dest) {
  if (variant === 'controllers-v2') {
    copyTree(src, dest)
    return
  }
  copyTree(join(src, 'core'), join(dest, 'core'))
  copyTree(join(src, 'adapters', 'apiproxy-v1'), join(dest, 'adapters', 'apiproxy-v1'))
}

function writeServerEntry() {
  if (variant === 'controllers-v2') {
    writeFileSync(join(outLib, 'server-entry.js'), "export * from './server/index.js'\n")
    writeFileSync(join(outLib, 'server-entry.d.ts'), "export * from './server/index.js'\n")
    return
  }
  writeFileSync(join(outLib, 'server-entry.js'), `import { createRequire } from 'node:module'
import { createApiRemoteAgentResolver } from '@deepseek-ai/dsh-api-remotes'
import { ApiProxyV1Backend, resolveApiProxyV1Runtime } from './server/adapters/apiproxy-v1/backend.js'
import { serve } from './server/core/serve.js'

export { ApiProxyV1Backend, resolveApiProxyV1Runtime }
export { serve }
export const name = 'tui-server-apiproxy-v1'
export const inject = ['apiProxy', 'agents', 'sessions', 'commands']

const requireFromRuntime = createRequire(import.meta.url)
const { toFetchHandler } = resolveApiProxyV1Runtime(requireFromRuntime)
const installedDsh = requireFromRuntime('@deepseek-ai/dsh/package.json')

export function apply(ctx, config = {}) {
  ctx.effect(() => {
    const fileReferences = ctx.get('fileReferences')
    const resolveAgent = createApiRemoteAgentResolver(ctx, {})
    const backend = new ApiProxyV1Backend({
      api: ctx.apiProxy,
      dshVersion: process.env.DSH_PAGER_EXPECTED_DSH_VERSION ?? installedDsh.version,
      toFetchHandler,
      extensions: {
        resolveAgent,
        commands: ctx.commands,
        ...(fileReferences === undefined ? {} : { fileReferences }),
      },
    })
    return serve(backend, config.input ?? process.stdin, config.output ?? process.stdout, {
      ...(config.maxQueuedFrames === undefined ? {} : { maxQueuedFrames: config.maxQueuedFrames }),
    })
  }, 'tui.serve')
}
`)
  writeFileSync(join(outLib, 'server-entry.d.ts'), `export { ApiProxyV1Backend, resolveApiProxyV1Runtime } from './server/adapters/apiproxy-v1/backend.js'
export { serve } from './server/core/serve.js'
export declare const name = "tui-server-apiproxy-v1"
export declare const inject: readonly ["apiProxy", "agents", "sessions", "commands"]
export declare function apply(ctx: any, config?: { input?: NodeJS.ReadableStream; output?: NodeJS.WritableStream; maxQueuedFrames?: number }): void
`)
}

function main() {
  run('pnpm', ['--filter', './packages/dsh-tui-*', 'run', 'build'])
  rmSync(outLib, { recursive: true, force: true })
  mkdirSync(outLib, { recursive: true })

  for (const pkg of PACKAGES) {
    const src = join(repoRoot, 'packages', pkg.dir, 'lib')
    if (!existsSync(src)) {
      fail(`missing ${src}`)
    }
    if (pkg.dest === 'server') copyServerTree(src, join(outLib, pkg.dest))
    else copyTree(src, join(outLib, pkg.dest))
  }

  for (const file of [...walkFiles(outLib, '.js'), ...walkFiles(outLib, '.d.ts')]) {
    const original = readFileSync(file, 'utf8')
    const rewritten = rewriteSpecifiers(original, file)
    if (rewritten !== original) {
      writeFileSync(file, rewritten)
    }
  }

  writeServerEntry()

  const embeddedPatch = readFileSync(join(repoRoot, config.patch), 'utf8')
  const runtimePatch = embeddedPatch.replaceAll(
    `name: '${config.patchServer}'`,
    `name: '${config.runtimeServer}'`,
  )
  if (runtimePatch === embeddedPatch) fail(`patch did not contain ${config.patchServer}`)
  writeFileSync(join(runtimeRoot, 'cordis.patch.yml'), runtimePatch)
  copyPackageLicenses(runtimeRoot)
  console.log(`assemble-runtime: wrote ${variant}`, outLib)
}

main()
