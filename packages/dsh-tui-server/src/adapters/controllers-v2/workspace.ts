import type { SessionId, WorkspaceView } from '@dsh-pager-grok/tui-protocol'
import type { WorkspaceFollowFrame } from './context.js'

export interface WorkspaceBaseline {
  items: WorkspaceView[]
  archivedSessionIds: SessionId[]
}

export function cloneWorkspaceBaseline(value: WorkspaceBaseline): WorkspaceBaseline {
  return {
    items: value.items.map(row => ({ ...row, sessionIds: [...row.sessionIds] })),
    archivedSessionIds: [...value.archivedSessionIds],
  }
}

/** Apply one controller delta to the adapter-owned workspace baseline cache. */
export function updateWorkspaceBaseline(
  baseline: WorkspaceBaseline,
  frame: Exclude<WorkspaceFollowFrame, { type: 'baseline' }>,
): void {
  if (frame.type === 'upsert') {
    const index = baseline.items.findIndex(row => row.workspaceId === frame.workspace.workspaceId)
    if (index < 0) baseline.items.push(frame.workspace)
    else baseline.items[index] = frame.workspace
  } else if (frame.type === 'remove') {
    baseline.items = baseline.items.filter(row => row.workspaceId !== frame.workspaceId)
  } else if (frame.type === 'order') {
    const rows = new Map(baseline.items.map(row => [row.workspaceId, row]))
    baseline.items = frame.workspaceIds
      .map(id => rows.get(id)).filter((row): row is WorkspaceView => row !== undefined)
  } else {
    baseline.archivedSessionIds = [...frame.archivedSessionIds]
  }
}
