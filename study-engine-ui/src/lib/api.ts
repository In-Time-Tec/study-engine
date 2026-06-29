import type { Stats, DueResponse, FetchDueOptions, QuestionsResponse, SessionsResponse, BankInfo, PendingSessionResponse, CreateGroupRoomResponse, GroupRoomState } from './types'
import {
  banksResponseSchema,
  certsResponseSchema,
  createGroupRoomResponseSchema,
  dueResponseSchema,
  groupRoomStateSchema,
  pendingSessionResponseSchema,
  questionsResponseSchema,
  sessionsResponseSchema,
  statsResponseSchema
} from './schemas'

const BASE = '/api'

function authHeaders(): Record<string, string> {
  const headers: Record<string, string> = {}
  const code = localStorage.getItem('accessCode')
  const user = localStorage.getItem('userName')
  if (code) headers['X-Access-Code'] = encodeURIComponent(code)
  if (user) headers['X-User'] = encodeURIComponent(user)
  return headers
}

async function checked(r: Response): Promise<unknown> {
  if (!r.ok) {
    const body = await r.json().catch(() => ({})) as Record<string, unknown>
    throw new Error((body['error'] as string) ?? `HTTP ${r.status}`)
  }
  return r.json()
}

export async function fetchCerts(): Promise<string[]> {
  const r = await fetch(`${BASE}/certs`, { headers: authHeaders() })
  return certsResponseSchema.parse(await checked(r)).certs
}

export async function fetchBanks(): Promise<BankInfo[]> {
  const r = await fetch(`${BASE}/banks`, { headers: authHeaders() })
  return banksResponseSchema.parse(await checked(r)).banks
}

// Result of an upload attempt. `conflict` signals a same-named bank already
// exists so the UI can confirm a replace and resend with overwrite=true,
// rather than surfacing a generic error.
export type UploadResult =
  | { ok: true; certs: string[] }
  | { ok: false; conflict: true }

export async function uploadBank(
  name: string,
  content: string,
  overwrite = false
): Promise<UploadResult> {
  const r = await fetch(`${BASE}/banks`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, content, overwrite })
  })
  if (r.status === 409) return { ok: false, conflict: true }
  return { ok: true, certs: certsResponseSchema.parse(await checked(r)).certs }
}

export async function deleteBank(name: string): Promise<string[]> {
  const r = await fetch(`${BASE}/banks/${encodeURIComponent(name)}`, { method: 'DELETE', headers: authHeaders() })
  return certsResponseSchema.parse(await checked(r)).certs
}

export async function fetchStats(cert = 'cca-f'): Promise<Stats> {
  const params = new URLSearchParams({ cert })
  const r = await fetch(`${BASE}/stats?${params}`, { headers: authHeaders() })
  return statsResponseSchema.parse(await checked(r))
}

export async function fetchDue({
  cert = 'cca-f',
  maxNew = 5,
  domain = null,
  tag = null,
  ids = null,
  all = false
}: FetchDueOptions = {}): Promise<DueResponse> {
  const params = new URLSearchParams({ cert, new: String(maxNew) })
  if (domain !== null) params.set('domain', String(domain))
  if (tag) params.set('tag', tag)
  if (ids && ids.length) params.set('ids', ids.join(','))
  if (all) params.set('all', 'true')
  const r = await fetch(`${BASE}/due?${params}`, { headers: authHeaders() })
  return dueResponseSchema.parse(await checked(r))
}

export async function fetchQuestions({
  cert = 'cca-f',
  domain = null,
  tag = null,
  search = null
}: {
  cert?: string
  domain?: number | null
  tag?: string | null
  search?: string | null
} = {}): Promise<QuestionsResponse> {
  const params = new URLSearchParams({ cert })
  if (domain !== null) params.set('domain', String(domain))
  if (tag) params.set('tag', tag)
  if (search) params.set('search', search)
  const r = await fetch(`${BASE}/questions?${params}`, { headers: authHeaders() })
  return questionsResponseSchema.parse(await checked(r))
}

export async function postReview({
  cardId,
  cert = 'cca-f',
  rating,
  isCorrect,
  selected
}: {
  cardId: string
  cert?: string
  rating: number
  isCorrect: boolean
  selected?: string | null
}): Promise<unknown> {
  const r = await fetch(`${BASE}/review`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ cardId, cert, rating, isCorrect, selected })
  })
  return checked(r)
}

export async function savePendingSession({
  cert,
  cardIds,
  controlMode,
  controlDomain
}: {
  cert: string
  cardIds: string[]
  controlMode: string
  controlDomain: number | null
}): Promise<void> {
  const r = await fetch(`${BASE}/pending-session`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ cert, cardIds, controlMode, controlDomain })
  })
  await checked(r)
}

export async function loadPendingSession(cert: string): Promise<PendingSessionResponse | null> {
  const params = new URLSearchParams({ cert })
  const r = await fetch(`${BASE}/pending-session?${params}`, { headers: authHeaders() })
  if (r.status === 404) return null
  return pendingSessionResponseSchema.parse(await checked(r))
}

export async function clearPendingSession(cert: string): Promise<void> {
  const params = new URLSearchParams({ cert })
  const r = await fetch(`${BASE}/pending-session?${params}`, { method: 'DELETE', headers: authHeaders() })
  await checked(r)
}

export async function postSession({
  cert = 'cca-f',
  total,
  correct
}: {
  cert?: string
  total: number
  correct: number
}): Promise<unknown> {
  const r = await fetch(`${BASE}/session`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ cert, total, correct })
  })
  return checked(r)
}

export async function fetchSessions({
  cert = 'cca-f',
  limit = 30
}: {
  cert?: string
  limit?: number
} = {}): Promise<SessionsResponse> {
  const params = new URLSearchParams({ cert, limit: String(limit) })
  const r = await fetch(`${BASE}/sessions?${params}`, { headers: authHeaders() })
  return sessionsResponseSchema.parse(await checked(r))
}

export async function createGroupRoom(cert = 'cca-f'): Promise<CreateGroupRoomResponse> {
  const r = await fetch(`${BASE}/group-rooms`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ cert })
  })
  return createGroupRoomResponseSchema.parse(await checked(r))
}

export async function fetchGroupRoom({
  code,
  participantId = null,
  hostToken = null
}: {
  code: string
  participantId?: string | null
  hostToken?: string | null
}): Promise<GroupRoomState> {
  const params = new URLSearchParams()
  if (participantId) params.set('participantId', participantId)
  const suffix = params.toString() ? `?${params}` : ''
  const r = await fetch(`${BASE}/group-rooms/${encodeURIComponent(code)}${suffix}`, {
    headers: hostToken ? { ...authHeaders(), 'X-Group-Host-Token': hostToken } : authHeaders()
  })
  return groupRoomStateSchema.parse(await checked(r))
}

export async function voteGroupRoom({
  code,
  participantId,
  answer
}: {
  code: string
  participantId: string
  answer: string
}): Promise<GroupRoomState> {
  const r = await fetch(`${BASE}/group-rooms/${encodeURIComponent(code)}/vote`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ participantId, answer })
  })
  return groupRoomStateSchema.parse(await checked(r))
}

async function postGroupHostAction(code: string, hostToken: string, action: 'reveal' | 'next' | 'prev' | 'end'): Promise<GroupRoomState> {
  const r = await fetch(`${BASE}/group-rooms/${encodeURIComponent(code)}/${action}`, {
    method: 'POST',
    headers: { ...authHeaders(), 'X-Group-Host-Token': hostToken }
  })
  return groupRoomStateSchema.parse(await checked(r))
}

export async function revealGroupRoom(code: string, hostToken: string): Promise<GroupRoomState> {
  return postGroupHostAction(code, hostToken, 'reveal')
}

export async function nextGroupRoom(code: string, hostToken: string): Promise<GroupRoomState> {
  return postGroupHostAction(code, hostToken, 'next')
}

/* istanbul ignore next */
export async function prevGroupRoom(code: string, hostToken: string): Promise<GroupRoomState> {
  return postGroupHostAction(code, hostToken, 'prev')
}

export async function endGroupRoom(code: string, hostToken: string): Promise<GroupRoomState> {
  return postGroupHostAction(code, hostToken, 'end')
}
