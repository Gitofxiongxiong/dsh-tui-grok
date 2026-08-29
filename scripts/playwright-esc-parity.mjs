#!/usr/bin/env node
/** Browser/xterm.js Esc comparison for DSH and a local Grok Build binary. */

import fs from 'node:fs/promises'
import path from 'node:path'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const playwrightModule = process.env.PLAYWRIGHT_MODULE || 'playwright'
const { chromium } = require(playwrightModule)

const args = new Map()
for (let index = 2; index < process.argv.length; index += 2) {
  args.set(process.argv[index], process.argv[index + 1])
}
const dshUrl = args.get('--dsh-url') || 'http://127.0.0.1:7681'
const grokUrl = args.get('--grok-url') || 'http://127.0.0.1:7683'
const outputDir = args.get('--output') || '/tmp/dsh-esc-playwright'
const executablePath = process.env.PLAYWRIGHT_CHROMIUM

await fs.mkdir(outputDir, { recursive: true })
const browser = await chromium.launch({
  headless: true,
  ...(executablePath ? { executablePath } : {}),
})

function terminalText(page) {
  return page.evaluate(() =>
    Array.from({ length: window.term.rows }, (_, row) =>
      window.term.buffer.active.getLine(row)?.translateToString(true) || '',
    ).join('\n'),
  )
}

async function waitForTerminal(page, predicate, label) {
  const deadline = Date.now() + 15_000
  while (Date.now() < deadline) {
    const text = await terminalText(page)
    if (predicate(text)) return text
    await page.waitForTimeout(50)
  }
  throw new Error(`${label}; terminal tail=${(await terminalText(page)).slice(-1200)}`)
}

async function openTerminal(url, name) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } })
  const errors = []
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text())
  })
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto(url)
  await page.waitForFunction(() => window.term && window.term.rows > 0)
  await page.locator('.xterm-helper-textarea').focus()
  await page.waitForTimeout(500)
  return { name, page, errors }
}

async function screenshot(terminal, name) {
  // xterm's buffer updates before its canvas animation frame; wait one frame
  // so pixel evidence cannot capture a partially painted delta.
  await terminal.page.waitForTimeout(100)
  await terminal.page.screenshot({ path: path.join(outputDir, `${name}.png`) })
}

try {
  const dsh = await openTerminal(dshUrl, 'dsh')
  const grok = await openTerminal(grokUrl, 'grok')
  await waitForTerminal(dsh.page, (text) => text.includes('Build anything'), 'DSH did not load')
  await waitForTerminal(grok.page, (text) => text.includes('Grok 4.6'), 'Grok did not load')

  for (const terminal of [dsh, grok]) {
    await terminal.page.keyboard.type('xraft-browser-clear')
    await waitForTerminal(
      terminal.page,
      (text) => text.includes('xraft-browser-clear'),
      `${terminal.name} did not render draft`,
    )
    await terminal.page.keyboard.press('Escape')
    await waitForTerminal(
      terminal.page,
      (text) => text.includes('press again to clear'),
      `${terminal.name} first Esc did not arm clear`,
    )
    await screenshot(terminal, `${terminal.name}-clear-arm`)
    await terminal.page.keyboard.press('Escape')
    await waitForTerminal(
      terminal.page,
      (text) => !text.includes('xraft-browser-clear') && !text.includes('press again to clear'),
      `${terminal.name} second Esc did not clear`,
    )
  }

  await dsh.page.keyboard.press('Escape')
  await dsh.page.waitForTimeout(100)
  if ((await terminalText(dsh.page)).includes('Rewind to which turn?')) {
    throw new Error('DSH first empty Esc was not a silent rewind arm')
  }
  await dsh.page.keyboard.press('Escape')
  await waitForTerminal(
    dsh.page,
    (text) => text.includes('Rewind to which turn?'),
    'DSH second empty Esc did not open rewind picker',
  )
  await screenshot(dsh, 'dsh-rewind-picker')
  await dsh.page.keyboard.press('Enter')
  await waitForTerminal(
    dsh.page,
    (text) => text.includes('Rewind conversation to'),
    'DSH rewind picker did not open Grok confirmation',
  )
  await screenshot(dsh, 'dsh-rewind-confirm')
  await dsh.page.keyboard.press('Escape')

  // This Grok fixture is a new blank Agent session. With no user turns both
  // Esc presses are swallowed; this pins the same no-history guard as DSH.
  await grok.page.keyboard.press('Escape')
  await grok.page.waitForTimeout(100)
  await grok.page.keyboard.press('Escape')
  await grok.page.waitForTimeout(200)
  const grokIdle = await terminalText(grok.page)
  if (grokIdle.includes('Rewind to which turn?') || grokIdle.includes('press again to clear')) {
    throw new Error('blank Grok session armed an idle action without draft/history')
  }
  await screenshot(grok, 'grok-idle-empty')

  const result = {
    viewport: { width: 1200, height: 800, deviceScaleFactor: 1 },
    dsh: {
      clearHint: true,
      rewindPicker: true,
      rewindConfirm: true,
      browserErrors: dsh.errors,
    },
    grok: {
      clearHint: true,
      emptyNoHistorySwallowed: true,
      browserErrors: grok.errors,
    },
    outputDir,
  }
  await fs.writeFile(path.join(outputDir, 'result.json'), `${JSON.stringify(result, null, 2)}\n`)
  console.log(JSON.stringify(result, null, 2))
  if (dsh.errors.length || grok.errors.length) process.exitCode = 1
} finally {
  await browser.close()
}
