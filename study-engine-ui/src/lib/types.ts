// Wire types are generated from the Rust structs (see study-engine-cli's
// `export_typescript_bindings` test) and re-exported here under the names the
// app already uses. This file owns only the UI-local types that have no
// backend equivalent. Do not hand-edit anything under `./generated`.
import type { Question } from './generated/Question'
import type { CardWithQuestion } from './generated/CardWithQuestion'

export type { Question } from './generated/Question'
export type { CardState } from './generated/CardState'
export type { CardWithQuestion } from './generated/CardWithQuestion'
export type { DueResponse } from './generated/DueResponse'
export type { QuestionsResponse } from './generated/QuestionsResponse'
export type { SessionsResponse } from './generated/SessionsResponse'
export type { DomainStat } from './generated/DomainStat'
export type { TagStat } from './generated/TagStat'
export type { StatsResponse as Stats } from './generated/StatsResponse'
export type { SessionItem as SessionRecord } from './generated/SessionItem'
export type { BankInfo } from './generated/BankInfo'
export type { BanksResponse } from './generated/BanksResponse'
export type { ReviewedCard } from './generated/ReviewedCard'
export type { PendingSessionResponse } from './generated/PendingSessionResponse'

// A study card is the backend's question+state pair. The bare-`Question` arm of
// the union is retained for callers that build plain cards; `isWrappedCard`
// (studySessionState.ts) discriminates on the presence of `cardState`.
export type WrappedCard = CardWithQuestion
export type QuestionEntry = CardWithQuestion
export type AnyCard = WrappedCard | Question

// ─── UI-only types (no backend equivalent) ──────────────────────────────────

export interface QuestionOptions {
  A?: string
  B?: string
  C?: string
  D?: string
}

export type StudyMode = 'due' | 'all' | 'custom'
export type StudyPhase = 'loading' | 'question' | 'revealed' | 'summary' | 'empty' | 'error'

export interface FetchDueOptions {
  cert?: string
  maxNew?: number
  domain?: number | null
  tag?: string | null
  ids?: string[] | null
  all?: boolean
}

export interface SessionResult {
  cardId: string
  isCorrect: boolean
  rating: number
  selected?: string | null
  domain?: number
  correctAnswer?: string
  questionText?: string
}
