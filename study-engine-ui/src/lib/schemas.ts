// Runtime validation for the HTTP API boundary.
//
// Each schema is pinned to its generated wire type with `satisfies
// z.ZodType<T>`, where T comes from `./generated` (emitted from the Rust
// structs — the single source of truth). If a Rust struct changes and the
// regenerated type no longer matches the schema here, this file fails to
// typecheck. That makes contract drift a compile error rather than a runtime
// surprise, and `api.ts` parses every response so malformed data is rejected
// at the boundary instead of silently flowing in as `undefined`.
import { z } from 'zod'
import type { Question } from './generated/Question'
import type { CardState } from './generated/CardState'
import type { CardWithQuestion } from './generated/CardWithQuestion'
import type { DomainStat } from './generated/DomainStat'
import type { TagStat } from './generated/TagStat'
import type { SessionItem } from './generated/SessionItem'
import type { DueResponse } from './generated/DueResponse'
import type { StatsResponse } from './generated/StatsResponse'
import type { QuestionsResponse } from './generated/QuestionsResponse'
import type { SessionsResponse } from './generated/SessionsResponse'
import type { CertsResponse } from './generated/CertsResponse'
import type { BankInfo } from './generated/BankInfo'
import type { BanksResponse } from './generated/BanksResponse'

export const questionSchema = z.object({
  id: z.string(),
  domain: z.number(),
  scenario: z.string(),
  question: z.string(),
  options: z.record(z.string(), z.string()),
  answer: z.string(),
  explanation: z.string(),
  tags: z.array(z.string())
}) satisfies z.ZodType<Question>

export const cardStateSchema = z.object({
  id: z.string(),
  cert: z.string(),
  stability: z.number().nullable(),
  difficulty: z.number().nullable(),
  due: z.string().nullable(),
  last_review: z.string().nullable(),
  reps: z.number()
}) satisfies z.ZodType<CardState>

export const cardWithQuestionSchema = z.object({
  question: questionSchema,
  cardState: cardStateSchema.nullable()
}) satisfies z.ZodType<CardWithQuestion>

export const domainStatSchema = z.object({
  id: z.number(),
  name: z.string(),
  total: z.number(),
  mastered: z.number(),
  reviewTotal: z.number(),
  reviewCorrect: z.number(),
  accuracy: z.number()
}) satisfies z.ZodType<DomainStat>

export const tagStatSchema = z.object({
  tag: z.string(),
  correct: z.number(),
  total: z.number(),
  accuracy: z.number()
}) satisfies z.ZodType<TagStat>

export const sessionItemSchema = z.object({
  date: z.string(),
  total: z.number(),
  correct: z.number(),
  accuracy: z.number()
}) satisfies z.ZodType<SessionItem>

export const dueResponseSchema = z.object({
  cards: z.array(cardWithQuestionSchema),
  dueCount: z.number(),
  newCount: z.number(),
  newRemaining: z.number(),
  mode: z.string()
}) satisfies z.ZodType<DueResponse>

export const statsResponseSchema = z.object({
  cert: z.string(),
  certName: z.string(),
  total: z.number(),
  introduced: z.number(),
  dueToday: z.number(),
  newAvailable: z.number(),
  mastered: z.number(),
  domains: z.array(domainStatSchema),
  tags: z.array(tagStatSchema),
  sessions: z.array(sessionItemSchema)
}) satisfies z.ZodType<StatsResponse>

export const questionsResponseSchema = z.object({
  cert: z.string(),
  certName: z.string(),
  domains: z.record(z.string(), z.string()),
  questions: z.array(cardWithQuestionSchema)
}) satisfies z.ZodType<QuestionsResponse>

export const sessionsResponseSchema = z.object({
  sessions: z.array(sessionItemSchema)
}) satisfies z.ZodType<SessionsResponse>

export const certsResponseSchema = z.object({
  certs: z.array(z.string())
}) satisfies z.ZodType<CertsResponse>

export const bankInfoSchema = z.object({
  name: z.string(),
  questionCount: z.number()
}) satisfies z.ZodType<BankInfo>

export const banksResponseSchema = z.object({
  banks: z.array(bankInfoSchema)
}) satisfies z.ZodType<BanksResponse>
