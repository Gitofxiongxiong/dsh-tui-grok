import { SessionId } from '@dsh-pager-grok/tui-protocol'

export const CONFORMANCE_SESSION_ID = SessionId('session-1')

export const SESSION_LIST_GOLDEN = {
  ok: true,
  value: {
    items: [{
      sessionId: CONFORMANCE_SESSION_ID,
      cwd: '/work',
      running: false,
      blank: false,
      agentPreset: 'standard',
      projections: { asOfSeq: 3, values: { agentPreset: 'standard' } },
    }],
  },
}

export const SESSION_SEARCH_GOLDEN = {
  ok: true,
  value: { items: [], query: 'needle' },
}

export const SESSION_CREATE_GOLDEN = {
  ok: true,
  value: { sessionId: CONFORMANCE_SESSION_ID },
}

export const WORKSPACE_BASELINE_GOLDEN = {
  ok: true,
  value: {
    items: [{
      workspaceId: 'workspace-1',
      path: '/work',
      title: 'Work',
      sessionIds: [CONFORMANCE_SESSION_ID],
      createdAt: '1',
      updatedAt: '1',
    }],
    archivedSessionIds: [],
  },
}
