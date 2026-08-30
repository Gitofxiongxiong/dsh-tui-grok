import { createRequire } from 'node:module'
import { dirname, resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { createApiRemoteAgentResolver } from '@deepseek-ai/dsh-api-remotes'

const fixtureDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = process.env.DSH_PAGER_GROK_ROOT ?? resolve(fixtureDir, '../../..')
const [{ ApiProxyV1Backend, resolveApiProxyV1Runtime }, { serve }] = await Promise.all([
  import(pathToFileURL(resolve(repoRoot, 'packages/dsh-tui-server/lib/adapters/apiproxy-v1/backend.js')).href),
  import(pathToFileURL(resolve(repoRoot, 'packages/dsh-tui-server/lib/core/serve.js')).href),
])

export const name = 'tui-server-apiproxy-v1'
export const inject = ['apiProxy', 'agents', 'sessions', 'commands']

const requireFromFixture = createRequire(import.meta.url)
const { toFetchHandler } = resolveApiProxyV1Runtime(requireFromFixture)
const dshVersion = process.env.DSH_COMPAT_VERSION ?? '0.1.0-rc.8'

/** Real rc.8 composition used only by the isolated compatibility fixture. */
export function apply(ctx, config = {}) {
  ctx.effect(() => {
    const fileReferences = ctx.get('fileReferences')
    const resolveAgent = createApiRemoteAgentResolver(ctx, {})
    const backend = new ApiProxyV1Backend({
      api: ctx.apiProxy,
      dshVersion,
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
