#!/usr/bin/env node
/**
 * Copy built protocol/server libs into @dsh-pager-grok/runtime and
 * rewrite in-workspace grok specifiers to relative paths so the published
 * tarball does not depend on workspace:* or the development TS packages.
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
const runtimeRoot = join(repoRoot, 'packages/dsh-pager-runtime')
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

function main() {
  run('pnpm', ['--filter', './packages/dsh-tui-*', 'run', 'build'])
  rmSync(outLib, { recursive: true, force: true })
  mkdirSync(outLib, { recursive: true })

  for (const pkg of PACKAGES) {
    const src = join(repoRoot, 'packages', pkg.dir, 'lib')
    if (!existsSync(src)) {
      fail(`missing ${src}`)
    }
    copyTree(src, join(outLib, pkg.dest))
  }

  for (const file of [...walkFiles(outLib, '.js'), ...walkFiles(outLib, '.d.ts')]) {
    const original = readFileSync(file, 'utf8')
    const rewritten = rewriteSpecifiers(original, file)
    if (rewritten !== original) {
      writeFileSync(file, rewritten)
    }
  }

  writeFileSync(
    join(outLib, 'server-entry.js'),
    "export * from './server/index.js'\n",
  )
  writeFileSync(
    join(outLib, 'server-entry.d.ts'),
    "export * from './server/index.js'\n",
  )

  const embeddedPatch = readFileSync(
    join(repoRoot, 'packages/dsh-tui-embedded/cordis.patch.yml'),
    'utf8',
  )
  const runtimePatch = embeddedPatch
    .replaceAll(
      "name: '@dsh-pager-grok/tui-server'",
      "name: '@dsh-pager-grok/runtime/server'",
    )
  writeFileSync(join(runtimeRoot, 'cordis.patch.yml'), runtimePatch)
  copyPackageLicenses(runtimeRoot)
  console.log('assemble-runtime: wrote', outLib)
}

main()
