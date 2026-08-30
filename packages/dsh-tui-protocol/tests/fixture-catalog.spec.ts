import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import {
  TUI_ERROR_CODES,
  TUI_NOTIFICATION_METHOD_SET,
  TUI_PROTOCOL_VERSION,
  TUI_REQUEST_METHOD_SET,
  TUI_UNARY_METHOD_SET,
} from '../src/index.ts'

const fixtures = join(dirname(fileURLToPath(import.meta.url)), 'fixtures')

function readFixture<T>(name: string): T {
  return JSON.parse(readFileSync(join(fixtures, name), 'utf8')) as T
}

describe('canonical protocol fixture catalog', () => {
  it('binds method, version, and error-kind fixtures to TypeScript constants', () => {
    const methods = readFixture<{
      unary: string[]
      control: string[]
      notification: string[]
    }>('method-catalog.json')
    const version = readFixture<{ tuiProtocolVersion: number }>('protocol-version.json')
    const errors = readFixture<{ errorKinds: string[] }>('error-kinds.json')

    expect(Object.keys(TUI_UNARY_METHOD_SET)).toEqual(methods.unary)
    expect(Object.keys(TUI_REQUEST_METHOD_SET)).toEqual(methods.control)
    expect(Object.keys(TUI_NOTIFICATION_METHOD_SET)).toEqual(methods.notification)
    expect(methods.unary.length + methods.control.length + methods.notification.length).toBe(65)
    expect(TUI_PROTOCOL_VERSION).toBe(version.tuiProtocolVersion)
    expect(Object.keys(TUI_ERROR_CODES)).toEqual(errors.errorKinds)
  })
})
