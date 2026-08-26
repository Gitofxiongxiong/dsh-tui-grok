/** Host → native optional package mapping. Keep in sync with scripts/pager-platform-matrix.json. */

export function detectLibc(env = process.env, report = process.report) {
  if (process.platform !== 'linux') {
    return null
  }
  try {
    const snapshot = report?.getReport?.()
    if (snapshot?.header && !snapshot.header.glibcVersionRuntime) {
      return 'musl'
    }
  } catch {
    // fall through
  }
  return env.DSH_PAGER_LIBC === 'musl' ? 'musl' : 'glibc'
}

export function nativeSpec(platform = process.platform, arch = process.arch, libc = detectLibc()) {
  if (platform === 'linux' && libc === 'musl') {
    return { error: 'musl/Alpine is not supported. Do not cargo build a gnu binary as a fallback.' }
  }
  const table = [
    { platform: 'linux', arch: 'x64', libc: 'glibc', name: '@dsh-pager-grok/native-linux-x64-gnu', bin: 'dsh-pager' },
    { platform: 'linux', arch: 'arm64', libc: 'glibc', name: '@dsh-pager-grok/native-linux-arm64-gnu', bin: 'dsh-pager' },
    { platform: 'darwin', arch: 'x64', libc: null, name: '@dsh-pager-grok/native-darwin-x64', bin: 'dsh-pager' },
    { platform: 'darwin', arch: 'arm64', libc: null, name: '@dsh-pager-grok/native-darwin-arm64', bin: 'dsh-pager' },
    { platform: 'win32', arch: 'x64', libc: null, name: '@dsh-pager-grok/native-win32-x64', bin: 'dsh-pager.exe' },
  ]
  const match = table.find(
    (row) =>
      row.platform === platform &&
      row.arch === arch &&
      (row.libc ?? null) === (platform === 'linux' ? libc : null),
  )
  if (!match) {
    return { error: `unsupported platform ${platform}-${arch}` }
  }
  return match
}

export function enginesSatisfied(version = process.versions.node) {
  const [major, minor] = version.split('.').map((part) => Number.parseInt(part, 10))
  if (major >= 24) return true
  if (major === 22 && minor >= 19) return true
  return false
}
