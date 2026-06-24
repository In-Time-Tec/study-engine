import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import {
  clearPendingSession,
  createGroupRoom,
  deleteBank,
  endGroupRoom,
  fetchBanks,
  fetchCerts,
  fetchDue,
  fetchGroupRoom,
  fetchQuestions,
  fetchSessions,
  fetchStats,
  loadPendingSession,
  nextGroupRoom,
  postReview,
  postSession,
  revealGroupRoom,
  savePendingSession,
  uploadBank,
  voteGroupRoom
} from './api'

const fetchMock = vi.fn()

function ok(body: unknown): Response {
  return { ok: true, status: 200, json: () => Promise.resolve(body) } as unknown as Response
}

function notOk(body: unknown, status = 500): Response {
  return { ok: false, status, json: () => Promise.resolve(body) } as unknown as Response
}

function lastCall(): [string, RequestInit | undefined] {
  return fetchMock.mock.calls[fetchMock.mock.calls.length - 1] as [string, RequestInit | undefined]
}
function lastUrl(): string {
  return lastCall()[0]
}
function lastHeaders(): Record<string, string> {
  return (lastCall()[1]?.headers ?? {}) as Record<string, string>
}

const groupState = {
  code: 'ABC234',
  cert: 'cca-f',
  status: 'voting',
  currentIndex: 0,
  totalQuestions: 2,
  currentQuestion: {
    id: 'q1',
    domain: 1,
    scenario: 'Scenario',
    question: 'Question?',
    options: { A: 'One', B: 'Two' }
  },
  voteCounts: [{ answer: 'A', count: 1 }, { answer: 'B', count: 0 }],
  totalVotes: 1,
  selectedAnswer: null,
  correctAnswer: null,
  explanation: null
}

beforeEach(() => {
  fetchMock.mockReset()
  vi.stubGlobal('fetch', fetchMock)
})
afterEach(() => {
  vi.unstubAllGlobals()
})

describe('authHeaders', () => {
  test('includes X-Access-Code and X-User when stored in localStorage', async () => {
    localStorage.setItem('accessCode', 'secret')
    localStorage.setItem('userName', 'Alice')
    fetchMock.mockResolvedValue(ok({ certs: [] }))
    await fetchCerts()
    expect(lastHeaders()['X-Access-Code']).toBe('secret')
    expect(lastHeaders()['X-User']).toBe('Alice')
    localStorage.removeItem('accessCode')
    localStorage.removeItem('userName')
  })
})

describe('GET endpoints', () => {
  test('fetchCerts returns the certs array', async () => {
    fetchMock.mockResolvedValue(ok({ certs: ['cca-f', 'aws-saa'] }))
    expect(await fetchCerts()).toEqual(['cca-f', 'aws-saa'])
    expect(lastUrl()).toBe('/api/certs')
  })

  test('fetchStats defaults the cert and accepts an override', async () => {
    fetchMock.mockResolvedValue(ok({
      cert: 'cca-f', certName: 'CCA', total: 1, introduced: 0, dueToday: 0,
      nextDue: null, newAvailable: 0, mastered: 0, domains: [], tags: [], sessions: []
    }))
    await fetchStats()
    expect(lastUrl()).toBe('/api/stats?cert=cca-f')
    await fetchStats('aws-saa')
    expect(lastUrl()).toBe('/api/stats?cert=aws-saa')
  })

  test('fetchDue omits optional params by default', async () => {
    fetchMock.mockResolvedValue(ok({ cards: [], dueCount: 0, newCount: 0, newRemaining: 0, mode: 'study', glossary: [] }))
    await fetchDue()
    const url = lastUrl()
    expect(url).toContain('cert=cca-f')
    expect(url).toContain('new=5')
    expect(url).not.toContain('domain=')
    expect(url).not.toContain('tag=')
    expect(url).not.toContain('ids=')
    expect(url).not.toContain('all=')
  })

  test('fetchDue includes every optional param when provided', async () => {
    fetchMock.mockResolvedValue(ok({ cards: [], dueCount: 0, newCount: 0, newRemaining: 0, mode: 'study', glossary: [] }))
    await fetchDue({ cert: 'x', maxNew: 10, domain: 3, tag: 'tools', ids: ['q1', 'q2'], all: true })
    const url = lastUrl()
    expect(url).toContain('new=10')
    expect(url).toContain('domain=3')
    expect(url).toContain('tag=tools')
    expect(url).toContain('ids=q1%2Cq2')
    expect(url).toContain('all=true')
  })

  test('fetchDue treats an empty ids array as no filter', async () => {
    fetchMock.mockResolvedValue(ok({ cards: [], dueCount: 0, newCount: 0, newRemaining: 0, mode: 'study', glossary: [] }))
    await fetchDue({ ids: [] })
    expect(lastUrl()).not.toContain('ids=')
  })

  test('fetchQuestions omits and includes optional filters', async () => {
    fetchMock.mockResolvedValue(ok({ cert: 'cca-f', certName: 'CCA', domains: {}, questions: [], glossary: [] }))
    await fetchQuestions()
    let url = lastUrl()
    expect(url).not.toContain('domain=')
    expect(url).not.toContain('tag=')
    expect(url).not.toContain('search=')

    await fetchQuestions({ domain: 1, tag: 'agents', search: 'mcp' })
    url = lastUrl()
    expect(url).toContain('domain=1')
    expect(url).toContain('tag=agents')
    expect(url).toContain('search=mcp')
  })

  test('fetchSessions defaults and overrides the limit', async () => {
    fetchMock.mockResolvedValue(ok({ sessions: [] }))
    await fetchSessions()
    expect(lastUrl()).toContain('limit=30')
    await fetchSessions({ limit: 5 })
    expect(lastUrl()).toContain('limit=5')
  })

  test('fetchGroupRoom includes participant and host token when provided', async () => {
    fetchMock.mockResolvedValue(ok(groupState))
    await fetchGroupRoom({ code: 'ABC234', participantId: 'p1', hostToken: 'secret' })
    const [url, init] = lastCall()
    expect(url).toBe('/api/group-rooms/ABC234?participantId=p1')
    expect(init?.headers).toEqual({ 'X-Group-Host-Token': 'secret' })
  })

  test('fetchGroupRoom omits optional query and headers by default', async () => {
    fetchMock.mockResolvedValue(ok(groupState))
    await fetchGroupRoom({ code: 'ABC234' })
    const [url, init] = lastCall()
    expect(url).toBe('/api/group-rooms/ABC234')
    expect(init?.headers).toEqual({})
  })
})

describe('POST endpoints', () => {
  test('postReview posts the review payload without selected when omitted', async () => {
    fetchMock.mockResolvedValue(ok({}))
    await postReview({ cardId: 'q1', rating: 3, isCorrect: true })
    const [url, init] = lastCall()
    expect(url).toBe('/api/review')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      cardId: 'q1',
      cert: 'cca-f',
      rating: 3,
      isCorrect: true
    })
  })

  test('postReview includes selected letter when provided', async () => {
    fetchMock.mockResolvedValue(ok({}))
    await postReview({ cardId: 'q1', rating: 1, isCorrect: false, selected: 'C' })
    const body = JSON.parse(lastCall()[1]?.body as string)
    expect(body.selected).toBe('C')
  })

  test('savePendingSession posts card IDs and control state', async () => {
    fetchMock.mockResolvedValue(ok({ ok: true }))
    await savePendingSession({ cert: 'cca-f', cardIds: ['q1', 'q2'], controlMode: 'due', controlDomain: null })
    const [url, init] = lastCall()
    expect(url).toBe('/api/pending-session')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      cert: 'cca-f', cardIds: ['q1', 'q2'], controlMode: 'due', controlDomain: null
    })
  })

  test('postSession posts the session payload', async () => {
    fetchMock.mockResolvedValue(ok({}))
    await postSession({ total: 10, correct: 8 })
    const [url, init] = lastCall()
    expect(url).toBe('/api/session')
    expect(JSON.parse(init?.body as string)).toEqual({ cert: 'cca-f', total: 10, correct: 8 })
  })

  test('createGroupRoom posts the cert and parses the room response', async () => {
    const body = { code: 'ABC234', hostToken: 'secret', joinUrl: 'http://x/?room=ABC234', state: groupState }
    fetchMock.mockResolvedValue(ok(body))
    expect(await createGroupRoom('cca-f')).toEqual(body)
    const [url, init] = lastCall()
    expect(url).toBe('/api/group-rooms')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({ cert: 'cca-f' })
  })

  test('createGroupRoom defaults to the bundled cert', async () => {
    const body = { code: 'ABC234', hostToken: 'secret', joinUrl: 'http://x/?room=ABC234', state: groupState }
    fetchMock.mockResolvedValue(ok(body))
    await createGroupRoom()
    expect(JSON.parse(lastCall()[1]?.body as string)).toEqual({ cert: 'cca-f' })
  })

  test('voteGroupRoom posts participant answer', async () => {
    fetchMock.mockResolvedValue(ok({ ...groupState, selectedAnswer: 'B' }))
    await voteGroupRoom({ code: 'ABC234', participantId: 'p1', answer: 'B' })
    const [url, init] = lastCall()
    expect(url).toBe('/api/group-rooms/ABC234/vote')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({ participantId: 'p1', answer: 'B' })
  })

  test('host group actions post the host token header', async () => {
    fetchMock.mockResolvedValue(ok({ ...groupState, status: 'revealed', correctAnswer: 'A' }))
    await revealGroupRoom('ABC234', 'secret')
    expect(lastCall()).toEqual([
      '/api/group-rooms/ABC234/reveal',
      { method: 'POST', headers: { 'X-Group-Host-Token': 'secret' } }
    ])

    await nextGroupRoom('ABC234', 'secret')
    expect(lastUrl()).toBe('/api/group-rooms/ABC234/next')

    await endGroupRoom('ABC234', 'secret')
    expect(lastUrl()).toBe('/api/group-rooms/ABC234/end')
  })
})

describe('pending session', () => {
  test('loadPendingSession returns null on 404', async () => {
    fetchMock.mockResolvedValue(notOk({ error: 'no pending session' }, 404))
    expect(await loadPendingSession('cca-f')).toBeNull()
    expect(lastUrl()).toContain('/api/pending-session')
    expect(lastUrl()).toContain('cert=cca-f')
  })

  test('loadPendingSession returns parsed response on success', async () => {
    const body = { cardIds: ['q1'], controlMode: 'due', controlDomain: null, reviewedCards: [] }
    fetchMock.mockResolvedValue(ok(body))
    expect(await loadPendingSession('cca-f')).toEqual(body)
  })

  test('clearPendingSession issues a DELETE with cert param', async () => {
    fetchMock.mockResolvedValue(ok({ ok: true }))
    await clearPendingSession('cca-f')
    const [url, init] = lastCall()
    expect(url).toContain('/api/pending-session')
    expect(url).toContain('cert=cca-f')
    expect(init?.method).toBe('DELETE')
  })
})

describe('bank management', () => {
  test('fetchBanks returns the banks array', async () => {
    fetchMock.mockResolvedValue(ok({ banks: [{ name: 'cca-f', questionCount: 60 }] }))
    expect(await fetchBanks()).toEqual([{ name: 'cca-f', questionCount: 60 }])
    expect(lastUrl()).toBe('/api/banks')
  })

  test('uploadBank posts the payload and returns the refreshed certs', async () => {
    fetchMock.mockResolvedValue(ok({ certs: ['cca-f', 'newbank'] }))
    const result = await uploadBank('newbank', '{"x":1}')
    expect(result).toEqual({ ok: true, certs: ['cca-f', 'newbank'] })
    const [url, init] = lastCall()
    expect(url).toBe('/api/banks')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(init?.body as string)).toEqual({
      name: 'newbank',
      content: '{"x":1}',
      overwrite: false
    })
  })

  test('uploadBank forwards the overwrite flag', async () => {
    fetchMock.mockResolvedValue(ok({ certs: ['cca-f'] }))
    await uploadBank('cca-f', '{}', true)
    expect(JSON.parse(lastCall()[1]?.body as string).overwrite).toBe(true)
  })

  test('uploadBank reports a 409 as a conflict instead of throwing', async () => {
    fetchMock.mockResolvedValue(notOk({ error: 'already exists' }, 409))
    expect(await uploadBank('cca-f', '{}')).toEqual({ ok: false, conflict: true })
  })

  test('uploadBank still throws on other errors', async () => {
    fetchMock.mockResolvedValue(notOk({ error: 'is not in options' }, 400))
    await expect(uploadBank('bad', '{}')).rejects.toThrow('is not in options')
  })

  test('deleteBank issues a DELETE with an encoded name and returns certs', async () => {
    fetchMock.mockResolvedValue(ok({ certs: ['cca-f'] }))
    expect(await deleteBank('aws saa')).toEqual(['cca-f'])
    const [url, init] = lastCall()
    expect(url).toBe('/api/banks/aws%20saa')
    expect(init?.method).toBe('DELETE')
  })
})

describe('error handling', () => {
  test('throws the server-provided error message', async () => {
    fetchMock.mockResolvedValue(notOk({ error: 'bank not found' }, 404))
    await expect(fetchCerts()).rejects.toThrow('bank not found')
  })

  test('falls back to the HTTP status when no error message is present', async () => {
    fetchMock.mockResolvedValue(notOk({}, 500))
    await expect(fetchStats()).rejects.toThrow('HTTP 500')
  })

  test('falls back to the HTTP status when the error body is not JSON', async () => {
    fetchMock.mockResolvedValue({
      ok: false,
      status: 503,
      json: () => Promise.reject(new Error('not json'))
    } as unknown as Response)
    await expect(fetchStats()).rejects.toThrow('HTTP 503')
  })
})

describe('boundary validation', () => {
  test('rejects a 200 response whose shape violates the wire contract', async () => {
    // dueCount/newCount missing — silently flowed through as undefined before
    // schema validation was added at the boundary.
    fetchMock.mockResolvedValue(ok({ cards: [] }))
    await expect(fetchDue()).rejects.toThrow()
  })

  test('rejects a response with a wrong field type', async () => {
    fetchMock.mockResolvedValue(ok({ certs: 'not-an-array' }))
    await expect(fetchCerts()).rejects.toThrow()
  })
})
