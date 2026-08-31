#!/usr/bin/env node
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { load } from 'js-yaml'

const workflowPath = fileURLToPath(
  new URL('../.github/workflows/release.yml', import.meta.url),
)
const workflowText = readFileSync(workflowPath, 'utf8')
const workflow = load(workflowText)
const jobs = workflow.jobs

test('derives the npm target tag only from the immutable release version', () => {
  const metadataRun = stepRun('metadata', 'validate immutable tag and package versions')

  assert.equal(
    jobs.metadata.outputs.target_tag,
    '${{ steps.release.outputs.target_tag }}',
  )
  assert.match(metadataRun, /target_tag='latest'/)
  assert.match(metadataRun, /target_tag='next'/)
  assert.match(metadataRun, /\^\[0-9\]\+\\\.\[0-9\]\+\\\.\[0-9\]\+\$/)
  assert.match(metadataRun, /echo "target_tag=\$\{target_tag\}"/)
  assert.doesNotMatch(workflowText, /npm\s+dist-tag/)
  assert.doesNotMatch(workflowText, /--tag\s+["']?release-candidate/)
  assert.doesNotMatch(workflowText, /dist-tags\.release-candidate/)
  assert.doesNotMatch(workflowText, /NODE_AUTH_TOKEN|NPM_TOKEN/)
})

test('publishes native, runtime and CLI through OIDC to the computed tag', () => {
  for (const [jobId, stepName] of [
    ['pack-native', 'publish native via OIDC'],
    ['publish-runtime', 'publish runtime via OIDC or verify exact candidate'],
    ['publish-cli', 'publish CLI via OIDC'],
  ]) {
    const step = namedStep(jobId, stepName)
    assert.equal(step.env.TARGET_TAG, '${{ needs.metadata.outputs.target_tag }}')
    assert.match(step.run, /--tag "\$\{TARGET_TAG\}" --provenance/)
    assert.match(step.run, /"dist-tags\.\$\{TARGET_TAG\}"/)
    assert.equal(jobs[jobId].permissions['id-token'], 'write')
  }
})

test('keeps the CLI last and gates the GitHub Release on the official CLI graph', () => {
  assert.deepEqual(jobs['publish-runtime'].needs, ['metadata', 'pack-native', 'pack-js'])
  assert.deepEqual(jobs['registry-cold'].needs, ['metadata', 'publish-runtime', 'pack-js'])
  assert.deepEqual(jobs['publish-cli'].needs, ['metadata', 'registry-cold', 'pack-js'])
  assert.deepEqual(jobs['verify-registry'].needs, ['metadata', 'publish-cli'])
  assert.deepEqual(jobs['registry-final'].needs, ['metadata', 'verify-registry'])
  assert.deepEqual(jobs['github-release'].needs, ['metadata', 'registry-final'])

  const finalRun = stepRun('registry-final', 'download official CLI and rehearse the public graph')
  assert.match(finalRun, /npm pack "\$\{package\}@\$\{VERSION\}"/)
  assert.match(finalRun, /DSH_RELEASE_CLI_TARBALL/)
  assert.match(finalRun, /scripts\/rehearse-release-candidate\.sh/)
  assert.match(finalRun, /result\.json/)

  const releaseRun = stepRun('github-release', 'create or verify the immutable Tag release')
  assert.match(releaseRun, /gh release view/)
  assert.match(releaseRun, /gh release create/)
  assert.match(releaseRun, /release_args\+=\(--latest\)/)
  assert.match(releaseRun, /release_args\+=\(--prerelease\)/)
})

test('grants registry and Release jobs only their required permissions', () => {
  assert.equal(workflow.permissions.contents, 'read')
  assert.equal(workflow.permissions.actions, 'read')
  assert.deepEqual(jobs['github-release'].permissions, { contents: 'write' })

  const contentWriters = Object.entries(jobs)
    .filter(([, job]) => job.permissions?.contents === 'write')
    .map(([jobId]) => jobId)
  assert.deepEqual(contentWriters, ['github-release'])
  assert.equal(jobs['registry-final'].permissions, undefined)
})

test('parses every explicit bash block after resolving GitHub expressions', () => {
  for (const [jobId, job] of Object.entries(jobs)) {
    for (const [index, step] of job.steps.entries()) {
      if (step.shell !== 'bash' || typeof step.run !== 'string') continue
      const script = step.run.replace(/\$\{\{[^}]*\}\}/g, 'github_expression')
      const result = spawnSync('bash', ['-n'], { input: script, encoding: 'utf8' })
      assert.equal(
        result.status,
        0,
        `${jobId} step ${index + 1} (${step.name ?? 'unnamed'}): ${result.stderr}`,
      )
    }
  }
})

function namedStep(jobId, stepName) {
  const step = jobs[jobId].steps.find((candidate) => candidate.name === stepName)
  assert.ok(step, `missing ${jobId} step: ${stepName}`)
  return step
}

function stepRun(jobId, stepName) {
  const run = namedStep(jobId, stepName).run
  assert.equal(typeof run, 'string')
  return run
}
