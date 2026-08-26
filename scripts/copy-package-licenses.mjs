#!/usr/bin/env node
/**
 * Copy LICENSE-MIT, LICENSE-APACHE, and NOTICE from the repo root into a
 * package directory so npm/pnpm pack includes them.
 */
import { copyFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const LICENSE_FILES = ['LICENSE-MIT', 'LICENSE-APACHE', 'NOTICE']

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

export function copyPackageLicenses(destDir, root = repoRoot) {
  for (const name of LICENSE_FILES) {
    copyFileSync(join(root, name), join(destDir, name))
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  copyPackageLicenses(resolve(process.cwd(), process.argv[2] ?? '.'))
}
