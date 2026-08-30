#!/usr/bin/env node
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)))
const require = createRequire(import.meta.url)
const releaseVersion = JSON.parse(readFileSync(join(repoRoot, 'package.json'), 'utf8')).version
const output = resolve(process.argv[2] ?? '')
if (!process.argv[2]) fail('usage: pack-release-candidates.mjs <empty-output-directory>')
mkdirSync(output, { recursive: true })
if (readdirSync(output).length > 0) fail(`output directory must be empty: ${output}`)

const units = [
  {
    id: 'protocol',
    dir: 'packages/dsh-tui-protocol',
    name: '@dsh-pager-grok/tui-protocol',
    publish: false,
    expected: ['package/lib/index.js'],
  },
  {
    id: 'server',
    dir: 'packages/dsh-tui-server',
    name: '@dsh-pager-grok/tui-server',
    publish: false,
    expected: ['package/lib/core/serve.js', 'package/lib/adapters/apiproxy-v1/backend.js'],
  },
  {
    id: 'runtime-apiproxy-v1',
    dir: 'packages/dsh-pager-runtime-apiproxy-v1',
    name: '@dsh-pager-grok/runtime-apiproxy-v1',
    publish: true,
    expected: ['package/lib/server-entry.js', 'package/cordis.patch.yml'],
  },
  {
    id: 'cli',
    dir: 'packages/dsh-pager-cli',
    name: '@dsh-pager-grok/cli',
    publish: true,
    expected: ['package/bin/dsh-pager.js', 'package/lib/dsh-support.json'],
  },
]

run(process.execPath, [join(repoRoot, 'scripts/pack-native.mjs'), '--pack-destination', output])
for (const unit of units) packWorkspace(unit)

const artifacts = []
for (const file of readdirSync(output).filter(name => name.endsWith('.tgz')).sort()) {
  const tarball = join(output, file)
  const manifest = JSON.parse(tar(['-xOf', tarball, 'package/package.json']))
  if (manifest.version !== releaseVersion) {
    fail(`${manifest.name}: version ${manifest.version} does not match product ${releaseVersion}`)
  }
  const unit = units.find(candidate => candidate.name === manifest.name)
  const native = manifest.name?.startsWith('@dsh-pager-grok/native-')
  if (unit === undefined && !native) fail(`unexpected tarball ${file}: ${manifest.name}`)
  const publish = native || unit.publish
  const listing = tar(['-tzf', tarball]).split('\n').filter(Boolean)
  for (const license of ['LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE']) {
    if (!listing.includes(`package/${license}`)) fail(`${manifest.name}: tarball is missing ${license}`)
  }
  for (const expected of unit?.expected ?? []) {
    if (!listing.includes(expected)) fail(`${manifest.name}: tarball is missing ${expected}`)
  }
  for (const section of ['dependencies', 'peerDependencies', 'optionalDependencies']) {
    for (const [name, specifier] of Object.entries(manifest[section] ?? {})) {
      if (/^(?:link|workspace):/.test(String(specifier))) {
        fail(`${manifest.name}: ${section}.${name} contains local specifier ${specifier}`)
      }
      if (publish && String(specifier).includes('alpha')) {
        fail(`${manifest.name}: published ${section}.${name} contains alpha specifier ${specifier}`)
      }
    }
  }
  if (publish && manifest.private === true) fail(`${manifest.name}: publish unit is private`)
  if (!publish && manifest.private !== true) fail(`${manifest.name}: bundled internal unit must remain private`)
  artifacts.push({
    id: native ? 'native-host' : unit.id,
    name: manifest.name,
    version: manifest.version,
    publish,
    tarball,
    bytes: readFileSync(tarball).byteLength,
  })
}

if (artifacts.length !== units.length + 1) {
  fail(`expected ${units.length + 1} tarballs, found ${artifacts.length}`)
}
const result = {
  schemaVersion: 1,
  productVersion: releaseVersion,
  generatedAt: new Date().toISOString(),
  artifacts,
  publishOrder: artifacts
    .filter(item => item.id === 'native-host' || item.id === 'runtime-apiproxy-v1' || item.id === 'cli')
    .sort((a, b) => ['native-host', 'runtime-apiproxy-v1', 'cli'].indexOf(a.id)
      - ['native-host', 'runtime-apiproxy-v1', 'cli'].indexOf(b.id))
    .map(item => item.name),
}
writeFileSync(join(output, 'release-candidates.json'), `${JSON.stringify(result, null, 2)}\n`)
process.stdout.write(`${JSON.stringify(result, null, 2)}\n`)

function packWorkspace(unit) {
  const npm = resolveNpmCli()
  run(process.execPath, [npm, 'pack', '--pack-destination', output], {
    cwd: join(repoRoot, unit.dir),
  })
}

function resolveNpmCli() {
  try {
    return require.resolve('npm/bin/npm-cli.js')
  } catch {
    // Node distributions normally place npm beside the executable.
  }
  const prefix = dirname(process.execPath)
  const candidates = [
    join(prefix, 'node_modules', 'npm', 'bin', 'npm-cli.js'),
    join(prefix, '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js'),
  ]
  const npm = candidates.find(existsSync)
  if (npm === undefined) fail('cannot resolve npm/bin/npm-cli.js next to node')
  return npm
}

function tar(args) {
  return run('tar', args, { capture: true }).stdout
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
    maxBuffer: 32 * 1024 * 1024,
  })
  if (result.error) fail(`${command}: ${result.error.message}`)
  if (result.status !== 0) fail(`${command} ${args.join(' ')} exited ${result.status}: ${result.stderr ?? ''}`)
  return result
}

function fail(message) {
  console.error(`pack-release-candidates: ${message}`)
  process.exit(1)
}
