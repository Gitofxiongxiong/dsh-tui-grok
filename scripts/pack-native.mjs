#!/usr/bin/env node
/**
 * Build, strip, and `npm pack` one `@dsh-pager-grok/native-*` tarball.
 *
 * Default: host platform. Override with `--id linux-x64-gnu` (must match
 * scripts/pager-platform-matrix.json). Uses `npm pack`, not `pnpm pack`,
 * because pnpm has been observed to strip the Unix executable bit.
 */
import { spawnSync } from 'node:child_process'
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, statSync } from 'node:fs'
import { createRequire } from 'node:module'
import { basename, dirname, isAbsolute, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { gunzipSync } from 'node:zlib'
import { copyPackageLicenses } from './copy-package-licenses.mjs'

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const matrixPath = join(repoRoot, 'scripts/pager-platform-matrix.json')

function fail(message) {
  console.error(`pack-native: ${message}`)
  process.exit(2)
}

const require = createRequire(import.meta.url)

function resolveNpmCli() {
  try {
    return require.resolve('npm/bin/npm-cli.js')
  } catch {
    // Node ships npm beside the executable. Never spawn PATH `npm` (Windows .cmd).
  }
  const prefix = dirname(process.execPath)
  const candidates = [
    join(prefix, 'node_modules', 'npm', 'bin', 'npm-cli.js'),
    join(prefix, '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js'),
  ]
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate
  }
  fail('cannot resolve npm/bin/npm-cli.js next to node; pack-native refuses PATH npm')
}

function runNpm(args, options = {}) {
  return run(process.execPath, [resolveNpmCli(), ...args], options)
}

function copyLicenseFiles(packageDir) {
  copyPackageLicenses(packageDir, repoRoot)
}

function tarCString(bytes) {
  const end = bytes.indexOf(0)
  return bytes.subarray(0, end === -1 ? bytes.length : end).toString('utf8').replace(/\0+$/g, '')
}

function tarOctal(bytes) {
  const text = tarCString(bytes).trim()
  if (!text) return 0
  const value = Number.parseInt(text, 8)
  return Number.isFinite(value) ? value : 0
}

/**
 * List gzip+ustar entries without PATH `tar`/`gzip`.
 * Windows GHA `tar.exe` shells out to gzip and fails from Node spawnSync.
 */
export function listGzipTarEntries(tarball) {
  let uncompressed
  try {
    uncompressed = gunzipSync(readFileSync(tarball))
  } catch (error) {
    fail(`cannot gunzip ${tarball}: ${error.message}`)
  }
  const entries = []
  let offset = 0
  while (offset + 512 <= uncompressed.length) {
    const header = uncompressed.subarray(offset, offset + 512)
    if (header.every((byte) => byte === 0)) {
      break
    }
    const name = tarCString(header.subarray(0, 100))
    const prefix = tarCString(header.subarray(345, 500))
    const fullName = prefix ? `${prefix}/${name}` : name
    const mode = tarOctal(header.subarray(100, 108))
    const size = tarOctal(header.subarray(124, 136))
    const typeflag = String.fromCharCode(header[156] || 0)
    entries.push({ name: fullName.replace(/\\/g, '/'), mode, typeflag, size })
    offset += 512 + Math.ceil(size / 512) * 512
  }
  return entries
}

function entryPath(entry) {
  return entry.name.replace(/^package\//, '')
}

function auditTarball(pkg, tarball) {
  const entries = listGzipTarEntries(tarball)
  for (const name of ['LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE']) {
    const found = entries.some((entry) => entryPath(entry) === name || entry.name.endsWith(`/${name}`))
    if (!found) {
      fail(`tarball is missing ${name}`)
    }
  }
  const expectedBin = `bin/${pkg.bin}`
  const bin = entries.find(
    (entry) => entryPath(entry) === expectedBin || entry.name.endsWith(`/${pkg.bin}`),
  )
  if (!bin) {
    fail(`tarball listing is missing ${expectedBin}`)
  }
  if (pkg.os === 'win32') {
    return
  }
  if ((bin.mode & 0o111) === 0) {
    fail(`tarball ${expectedBin} is not executable: mode=${bin.mode.toString(8)}`)
  }
}

function packInfo(parsed, pkg) {
  if (Array.isArray(parsed)) {
    return parsed[0]
  }
  if (parsed && typeof parsed === 'object' && parsed.filename) {
    return parsed
  }
  if (parsed && typeof parsed === 'object' && parsed[pkg.npm]) {
    return parsed[pkg.npm]
  }
  const values = parsed && typeof parsed === 'object' ? Object.values(parsed) : []
  if (values.length === 1 && values[0] && typeof values[0] === 'object') {
    return values[0]
  }
  fail(`npm pack JSON missing ${pkg.npm} info:\n${JSON.stringify(parsed)}`)
}

function packedTarballPath(info, pkg) {
  const fallback = `${pkg.npm.replace('@', '').replace('/', '-')}-${info.version}.tgz`
  const filename = info.filename || fallback
  if (isAbsolute(filename) && existsSync(filename)) {
    return filename
  }
  const fromRoot = join(repoRoot, basename(filename))
  if (existsSync(fromRoot)) {
    return fromRoot
  }
  fail(`packed tarball not found at ${fromRoot} (filename=${filename})`)
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  })
  if (result.error) {
    fail(`${command} ${args.join(' ')}: ${result.error.message}`)
  }
  if (result.status !== 0) {
    const stderr = (result.stderr || '').trim()
    fail(`${command} ${args.join(' ')} exited ${result.status}${stderr ? `\n${stderr}` : ''}`)
  }
  return result
}

function detectLibc() {
  if (process.platform !== 'linux') {
    return null
  }
  try {
    const report = process.report?.getReport?.()
    if (report?.header && !report.header.glibcVersionRuntime) {
      return 'musl'
    }
  } catch {
    // fall through to ldd
  }
  const ldd = spawnSync('ldd', ['--version'], { encoding: 'utf8' })
  const text = `${ldd.stdout || ''}\n${ldd.stderr || ''}`.toLowerCase()
  if (text.includes('musl')) {
    return 'musl'
  }
  return 'glibc'
}

function hostPackageId(matrix) {
  const libc = detectLibc()
  if (process.platform === 'linux' && libc === 'musl') {
    fail('musl/Alpine is not supported. Do not pack a gnu binary as a fallback.')
  }
  const match = matrix.packages.find(
    (pkg) =>
      pkg.os === process.platform &&
      pkg.cpu === process.arch &&
      (pkg.libc ?? null) === (pkg.os === 'linux' ? libc : null),
  )
  if (!match) {
    fail(`unsupported host ${process.platform}-${process.arch}${libc ? ` (${libc})` : ''}`)
  }
  return match.id
}

function parseArgs(argv) {
  let id = null
  let skipBuild = false
  let packDestination = repoRoot
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === '--id') {
      id = argv[index + 1]
      index += 1
      if (!id) {
        fail('--id needs a matrix package id')
      }
    } else if (arg === '--skip-build') {
      skipBuild = true
    } else if (arg === '--pack-destination') {
      packDestination = resolve(argv[index + 1] ?? '')
      index += 1
      if (!argv[index]) fail('--pack-destination needs a directory')
    } else if (arg === '-h' || arg === '--help') {
      console.log('Usage: node scripts/pack-native.mjs [--id <matrix-id>] [--skip-build] [--pack-destination <dir>]')
      process.exit(0)
    } else {
      fail(`unknown argument: ${arg}`)
    }
  }
  return { id, skipBuild, packDestination }
}

function cargoBinPath(pkg, usedTarget) {
  if (usedTarget) {
    return join(repoRoot, 'target', pkg.rustcTarget, 'release', pkg.bin)
  }
  return join(repoRoot, 'target/release', pkg.bin)
}

function stripBinary(pkg, binary) {
  if (pkg.os === 'win32') {
    return
  }
  const strip = spawnSync('strip', [binary], { encoding: 'utf8' })
  if (strip.status !== 0) {
    fail(`strip ${binary} failed: ${(strip.stderr || strip.stdout || '').trim()}`)
  }
}

function main() {
  const matrix = JSON.parse(readFileSync(matrixPath, 'utf8'))
  const args = parseArgs(process.argv.slice(2))
  const id = args.id ?? hostPackageId(matrix)
  const pkg = matrix.packages.find((entry) => entry.id === id)
  if (!pkg) {
    fail(`unknown package id ${id}`)
  }
  if (pkg.os === 'linux' && detectLibc() === 'musl' && !args.id) {
    fail('musl/Alpine is not supported')
  }

  const cargoArgs = ['build', '-p', 'dsh-pager-bin', '--release', '--locked']
  const hostTriple = run('rustc', ['-vV']).stdout.match(/host: (\S+)/)?.[1]
  const usedTarget = Boolean(hostTriple && hostTriple !== pkg.rustcTarget)
  if (usedTarget) {
    cargoArgs.push('--target', pkg.rustcTarget)
  }
  if (!args.skipBuild) {
    run('cargo', cargoArgs, { stdio: 'inherit' })
  }

  const built = cargoBinPath(pkg, usedTarget)
  try {
    statSync(built)
  } catch {
    fail(`missing release binary ${built}; run without --skip-build`)
  }

  const packageDir = join(repoRoot, pkg.dir)
  const destDir = join(packageDir, 'bin')
  mkdirSync(destDir, { recursive: true })
  const dest = join(destDir, pkg.bin)
  copyFileSync(built, dest)
  if (pkg.os !== 'win32') {
    chmodSync(dest, 0o755)
  }
  stripBinary(pkg, dest)
  copyLicenseFiles(packageDir)

  mkdirSync(args.packDestination, { recursive: true })
  const pack = runNpm(['pack', '--json', '--pack-destination', args.packDestination], {
    cwd: packageDir,
  })
  let parsed
  try {
    parsed = JSON.parse(pack.stdout)
  } catch {
    fail(`npm pack did not print JSON:\n${pack.stdout}`)
  }
  const info = packInfo(parsed, pkg)
  const filename = info.filename || `${pkg.npm.replace('@', '').replace('/', '-')}-${info.version}.tgz`
  const tarball = isAbsolute(filename)
    ? filename
    : join(args.packDestination, basename(filename))
  if (!existsSync(tarball)) fail(`packed tarball not found: ${tarball}`)

  const names = (info.files || []).map((file) => (typeof file === 'string' ? file : file.path))
  const expectedBin = `bin/${pkg.bin}`
  if (!names.includes(expectedBin) && !names.includes(`package/${expectedBin}`)) {
    // npm pack --json file paths are tarball-relative without package/
    const hasBin = names.some((name) => name === expectedBin || name.endsWith(`/${pkg.bin}`))
    if (!hasBin) {
      fail(`tarball is missing ${expectedBin}: ${names.join(', ')}`)
    }
  }
  if (pkg.os !== 'win32') {
    const mode = statSync(dest).mode
    if ((mode & 0o111) === 0) {
      fail(`${dest} is not executable`)
    }
  }
  auditTarball(pkg, tarball)

  const size = info.size ?? statSync(tarball).size
  console.log(
    JSON.stringify(
      {
        id: pkg.id,
        npm: pkg.npm,
        tarball,
        size,
        unpackedSize: info.unpackedSize,
        files: names,
      },
      null,
      2,
    ),
  )
}

const invokedDirectly =
  Boolean(process.argv[1]) && fileURLToPath(import.meta.url) === resolve(process.argv[1])
if (invokedDirectly) {
  try {
    main()
  } catch (error) {
    fail(error?.stack || String(error))
  }
}
