#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'

const GRAPHQL_ENDPOINT = 'https://api.github.com/graphql'
const DAY_MS = 24 * 60 * 60 * 1000

function parseArgs(argv) {
  const options = {
    repo: process.env.GITHUB_REPOSITORY,
    outLight: 'docs/assets/readme/star-history.svg',
    outDark: 'docs/assets/readme/star-history-dark.svg',
  }

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--help' || argument === '-h') {
      process.stdout.write(
        [
          'Usage: node scripts/generate-star-history.mjs [options]',
          '',
          'Options:',
          '  --repo OWNER/REPO   Repository to chart (default: GITHUB_REPOSITORY)',
          '  --out-light PATH    Light SVG output path',
          '  --out-dark PATH     Dark SVG output path',
          '',
          'Authentication: set GITHUB_TOKEN or GH_TOKEN.',
          '',
        ].join('\n'),
      )
      process.exit(0)
    }

    const nextValue = argv[index + 1]
    if (argument === '--repo' && nextValue) {
      options.repo = nextValue
      index += 1
    } else if (argument === '--out-light' && nextValue) {
      options.outLight = nextValue
      index += 1
    } else if (argument === '--out-dark' && nextValue) {
      options.outDark = nextValue
      index += 1
    } else {
      throw new Error(`Unknown or incomplete argument: ${argument}`)
    }
  }

  if (!options.repo || !options.repo.includes('/')) {
    throw new Error('Repository must be provided as OWNER/REPO via --repo or GITHUB_REPOSITORY')
  }

  const [owner, name, ...rest] = options.repo.split('/')
  if (!owner || !name || rest.length > 0) {
    throw new Error(`Invalid repository name: ${options.repo}`)
  }

  return { ...options, owner, name }
}

async function graphql(token, query, variables) {
  const response = await fetch(GRAPHQL_ENDPOINT, {
    method: 'POST',
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${token}`,
      'content-type': 'application/json',
      'user-agent': 'dsh-tui-grok-star-history',
      'x-github-api-version': '2022-11-28',
    },
    body: JSON.stringify({ query, variables }),
  })

  const payload = await response.json().catch(() => null)
  if (!response.ok) {
    throw new Error(`GitHub GraphQL request failed (${response.status}): ${payload?.message ?? 'unknown error'}`)
  }
  if (payload?.errors?.length) {
    throw new Error(`GitHub GraphQL error: ${payload.errors.map((error) => error.message).join('; ')}`)
  }
  return payload.data
}

async function fetchRepositoryStars(token, owner, name) {
  const query = `
    query StarHistory($owner: String!, $name: String!, $cursor: String) {
      repository(owner: $owner, name: $name) {
        createdAt
        nameWithOwner
        stargazerCount
        stargazers(first: 100, after: $cursor) {
          edges {
            starredAt
          }
          pageInfo {
            endCursor
            hasNextPage
          }
        }
      }
    }
  `

  const starredAt = []
  let cursor = null
  let repository = null

  do {
    const data = await graphql(token, query, { owner, name, cursor })
    repository = data.repository
    if (!repository) {
      throw new Error(`Repository not found or token cannot read it: ${owner}/${name}`)
    }

    for (const edge of repository.stargazers.edges) {
      if (edge?.starredAt) starredAt.push(edge.starredAt)
    }

    const { hasNextPage, endCursor } = repository.stargazers.pageInfo
    cursor = hasNextPage ? endCursor : null
    if (hasNextPage && !endCursor) {
      throw new Error('GitHub returned an incomplete stargazer pagination cursor')
    }
  } while (cursor)

  starredAt.sort()
  if (starredAt.length !== repository.stargazerCount) {
    throw new Error(
      `Expected ${repository.stargazerCount} stargazers but received ${starredAt.length}; refusing to draw an incomplete chart`,
    )
  }

  return {
    createdAt: repository.createdAt,
    nameWithOwner: repository.nameWithOwner,
    stargazerCount: repository.stargazerCount,
    starredAt,
  }
}

function utcDay(value) {
  const date = new Date(value)
  return Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate())
}

function buildPoints(repository) {
  const starsPerDay = new Map()
  for (const timestamp of repository.starredAt) {
    const day = utcDay(timestamp)
    starsPerDay.set(day, (starsPerDay.get(day) ?? 0) + 1)
  }

  if (starsPerDay.size === 0) {
    const createdDay = utcDay(repository.createdAt)
    return [
      { time: createdDay, count: 0 },
      { time: createdDay + DAY_MS, count: 0 },
    ]
  }

  const days = [...starsPerDay.keys()].sort((left, right) => left - right)
  const points = [{ time: days[0] - DAY_MS, count: 0 }]
  let cumulative = 0
  for (const day of days) {
    cumulative += starsPerDay.get(day)
    points.push({ time: day, count: cumulative })
  }
  return points
}

function niceScale(maximum, targetTicks = 5) {
  if (maximum <= 0) return { maximum: 1, step: 1 }
  const roughStep = maximum / targetTicks
  const magnitude = 10 ** Math.floor(Math.log10(roughStep))
  const normalized = roughStep / magnitude
  const multiplier = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10
  const step = multiplier * magnitude
  return { maximum: Math.ceil(maximum / step) * step, step }
}

function escapeXml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;')
}

function formatDate(timestamp, spanDays) {
  const date = new Date(timestamp)
  const year = date.getUTCFullYear()
  const month = String(date.getUTCMonth() + 1).padStart(2, '0')
  const day = String(date.getUTCDate()).padStart(2, '0')
  if (spanDays > 730) return String(year)
  if (spanDays > 90) return `${year}-${month}`
  return `${month}-${day}`
}

function xTicks(minimum, maximum) {
  const spanDays = Math.max(1, Math.round((maximum - minimum) / DAY_MS))
  const tickCount = Math.min(6, spanDays + 1)
  const ticks = []
  for (let index = 0; index < tickCount; index += 1) {
    const ratio = tickCount === 1 ? 0 : index / (tickCount - 1)
    const time = minimum + (maximum - minimum) * ratio
    const label = formatDate(time, spanDays)
    if (!ticks.some((tick) => tick.label === label)) ticks.push({ time, label })
  }
  return ticks
}

function renderSvg(repository, theme) {
  const colors =
    theme === 'dark'
      ? {
          background: '#0d1117',
          border: '#30363d',
          grid: '#30363d',
          muted: '#8b949e',
          text: '#e6edf3',
          accent: '#58a6ff',
          accentSoft: '#1f6feb',
          badge: '#161b22',
        }
      : {
          background: '#ffffff',
          border: '#d0d7de',
          grid: '#d8dee4',
          muted: '#57606a',
          text: '#1f2328',
          accent: '#0969da',
          accentSoft: '#54aeff',
          badge: '#f6f8fa',
        }

  const width = 900
  const height = 500
  const margin = { top: 92, right: 36, bottom: 66, left: 72 }
  const plotWidth = width - margin.left - margin.right
  const plotHeight = height - margin.top - margin.bottom
  const points = buildPoints(repository)
  const minimumTime = points[0].time
  const maximumTime = points.at(-1).time
  const { maximum: maximumStars, step } = niceScale(repository.stargazerCount)
  const scaleX = (time) => margin.left + ((time - minimumTime) / (maximumTime - minimumTime)) * plotWidth
  const scaleY = (count) => margin.top + plotHeight - (count / maximumStars) * plotHeight
  const coordinates = points.map((point) => ({ x: scaleX(point.time), y: scaleY(point.count) }))
  const linePath = coordinates
    .map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`)
    .join(' ')
  const areaPath = `${linePath} L ${coordinates.at(-1).x.toFixed(2)} ${(margin.top + plotHeight).toFixed(2)} L ${coordinates[0].x.toFixed(2)} ${(margin.top + plotHeight).toFixed(2)} Z`
  const lastPoint = coordinates.at(-1)

  const horizontalGrid = []
  for (let value = 0; value <= maximumStars + step / 2; value += step) {
    const y = scaleY(value)
    horizontalGrid.push(
      `<line x1="${margin.left}" y1="${y.toFixed(2)}" x2="${margin.left + plotWidth}" y2="${y.toFixed(2)}" stroke="${colors.grid}" stroke-width="1" opacity="0.75" />`,
      `<text x="${margin.left - 14}" y="${(y + 4).toFixed(2)}" text-anchor="end" fill="${colors.muted}" font-size="12">${value}</text>`,
    )
  }

  const verticalGrid = xTicks(minimumTime, maximumTime).flatMap(({ time, label }) => {
    const x = scaleX(time)
    return [
      `<line x1="${x.toFixed(2)}" y1="${margin.top}" x2="${x.toFixed(2)}" y2="${margin.top + plotHeight}" stroke="${colors.grid}" stroke-width="1" opacity="0.45" />`,
      `<text x="${x.toFixed(2)}" y="${margin.top + plotHeight + 28}" text-anchor="middle" fill="${colors.muted}" font-size="12">${escapeXml(label)}</text>`,
    ]
  })

  const title = escapeXml(repository.nameWithOwner)
  const count = repository.stargazerCount.toLocaleString('en-US')
  return [
    '<svg xmlns="http://www.w3.org/2000/svg" width="900" height="500" viewBox="0 0 900 500" role="img">',
    `  <title>${title} GitHub Star history</title>`,
    `  <desc>Cumulative GitHub Stars over time. Current total: ${count}.</desc>`,
    '  <defs>',
    '    <linearGradient id="star-area" x1="0" y1="0" x2="0" y2="1">',
    `      <stop offset="0%" stop-color="${colors.accentSoft}" stop-opacity="0.32" />`,
    `      <stop offset="100%" stop-color="${colors.accentSoft}" stop-opacity="0.03" />`,
    '    </linearGradient>',
    '  </defs>',
    `  <rect x="0.5" y="0.5" width="899" height="499" rx="12" fill="${colors.background}" stroke="${colors.border}" />`,
    `  <text x="${margin.left}" y="38" fill="${colors.text}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="20" font-weight="600">GitHub Star History</text>`,
    `  <text x="${margin.left}" y="64" fill="${colors.muted}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="14">${title}</text>`,
    `  <rect x="${width - margin.right - 118}" y="24" width="118" height="44" rx="9" fill="${colors.badge}" stroke="${colors.border}" />`,
    `  <text x="${width - margin.right - 104}" y="42" fill="${colors.muted}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="11">TOTAL STARS</text>`,
    `  <text x="${width - margin.right - 104}" y="61" fill="${colors.text}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="18" font-weight="700">${count}</text>`,
    ...horizontalGrid.map((line) => `  ${line}`),
    ...verticalGrid.map((line) => `  ${line}`),
    `  <path d="${areaPath}" fill="url(#star-area)" />`,
    `  <path d="${linePath}" fill="none" stroke="${colors.accent}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />`,
    `  <circle cx="${lastPoint.x.toFixed(2)}" cy="${lastPoint.y.toFixed(2)}" r="5" fill="${colors.background}" stroke="${colors.accent}" stroke-width="3" />`,
    `  <text x="${margin.left + plotWidth / 2}" y="${height - 18}" text-anchor="middle" fill="${colors.muted}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="12">Date (UTC)</text>`,
    `  <text x="18" y="${margin.top + plotHeight / 2}" text-anchor="middle" fill="${colors.muted}" font-family="ui-sans-serif, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif" font-size="12" transform="rotate(-90 18 ${margin.top + plotHeight / 2})">Stars</text>`,
    '</svg>',
    '',
  ].join('\n')
}

function writeIfChanged(filePath, content) {
  const absolutePath = path.resolve(filePath)
  fs.mkdirSync(path.dirname(absolutePath), { recursive: true })
  if (fs.existsSync(absolutePath) && fs.readFileSync(absolutePath, 'utf8') === content) return false
  fs.writeFileSync(absolutePath, content)
  return true
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN
  if (!token) throw new Error('GITHUB_TOKEN or GH_TOKEN is required')

  const repository = await fetchRepositoryStars(token, options.owner, options.name)
  const lightChanged = writeIfChanged(options.outLight, renderSvg(repository, 'light'))
  const darkChanged = writeIfChanged(options.outDark, renderSvg(repository, 'dark'))
  const state = lightChanged || darkChanged ? 'updated' : 'unchanged'
  process.stdout.write(
    `${state}: ${repository.nameWithOwner} (${repository.stargazerCount} stars) -> ${options.outLight}, ${options.outDark}\n`,
  )
}

main().catch((error) => {
  process.stderr.write(`${error.stack ?? error.message}\n`)
  process.exitCode = 1
})
