import type { Stats, DueResponse, FetchDueOptions, QuestionsResponse, SessionsResponse, BankInfo } from './types'
import {
  banksResponseSchema,
  certsResponseSchema,
  dueResponseSchema,
  questionsResponseSchema,
  sessionsResponseSchema,
  statsResponseSchema
} from './schemas'

const BASE = '/api'

async function checked(r: Response): Promise<unknown> {
  if (!r.ok) {
    const body = await r.json().catch(() => ({})) as Record<string, unknown>
    throw new Error((body['error'] as string) ?? `HTTP ${r.status}`)
  }
  return r.json()
}

export async function fetchCerts(): Promise<string[]> {
  const r = await fetch(`${BASE}/certs`)
  return certsResponseSchema.parse(await checked(r)).certs
}

export async function fetchBanks(): Promise<BankInfo[]> {
  const r = await fetch(`${BASE}/banks`)
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
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, content, overwrite })
  })
  if (r.status === 409) return { ok: false, conflict: true }
  return { ok: true, certs: certsResponseSchema.parse(await checked(r)).certs }
}

export async function deleteBank(name: string): Promise<string[]> {
  const r = await fetch(`${BASE}/banks/${encodeURIComponent(name)}`, { method: 'DELETE' })
  return certsResponseSchema.parse(await checked(r)).certs
}

export async function fetchStats(cert = 'cca-f'): Promise<Stats> {
  const params = new URLSearchParams({ cert })
  const r = await fetch(`${BASE}/stats?${params}`)
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
  const r = await fetch(`${BASE}/due?${params}`)
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
  const r = await fetch(`${BASE}/questions?${params}`)
  return questionsResponseSchema.parse(await checked(r))
}

export async function postReview({
  cardId,
  cert = 'cca-f',
  rating,
  isCorrect
}: {
  cardId: string
  cert?: string
  rating: number
  isCorrect: boolean
}): Promise<unknown> {
  const r = await fetch(`${BASE}/review`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cardId, cert, rating, isCorrect })
  })
  return checked(r)
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
    headers: { 'Content-Type': 'application/json' },
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
  const r = await fetch(`${BASE}/sessions?${params}`)
  return sessionsResponseSchema.parse(await checked(r))
}
