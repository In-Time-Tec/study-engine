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
import type { QuestionSource } from './generated/QuestionSource'
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
import type { ReviewedCard } from './generated/ReviewedCard'
import type { PendingSessionResponse } from './generated/PendingSessionResponse'
import type { GlossaryEntry } from './generated/GlossaryEntry'
import type { GroupQuestion } from './generated/GroupQuestion'
import type { GroupVoteCount } from './generated/GroupVoteCount'
import type { GroupRoomState } from './generated/GroupRoomState'
import type { CreateGroupRoomResponse } from './generated/CreateGroupRoomResponse'

export const questionSourceSchema = z.object({
  url: z.string(),
  quote: z.string(),
  confidence: z.string(),
  issues: z.array(z.string())
}) satisfies z.ZodType<QuestionSource>

export const questionSchema = z.object({
  id: z.string(),
  domain: z.number(),
  scenario: z.string(),
  question: z.string(),
  options: z.record(z.string(), z.string()),
  answer: z.string(),
  explanation: z.string(),
  tags: z.array(z.string()),
  glossaryExclude: z.array(z.string()),
  source: questionSourceSchema.nullable().optional()
}) satisfies z.ZodType<Question>

export const glossaryEntrySchema = z.object({
  term: z.string(),
  aliases: z.array(z.string()),
  definition: z.string(),
  sourceUrl: z.string(),
  sourceTitle: z.string().nullable()
}) satisfies z.ZodType<GlossaryEntry>

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
  mode: z.string(),
  glossary: z.array(glossaryEntrySchema)
}) satisfies z.ZodType<DueResponse>

export const statsResponseSchema = z.object({
  cert: z.string(),
  certName: z.string(),
  total: z.number(),
  introduced: z.number(),
  dueToday: z.number(),
  nextDue: z.string().nullable(),
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
  questions: z.array(cardWithQuestionSchema),
  glossary: z.array(glossaryEntrySchema)
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

export const reviewedCardSchema = z.object({
  cardId: z.string(),
  isCorrect: z.boolean(),
  rating: z.number(),
  selectedLetter: z.string().nullable(),
  domain: z.number(),
  correctAnswer: z.string(),
  questionText: z.string()
}) satisfies z.ZodType<ReviewedCard>

export const pendingSessionResponseSchema = z.object({
  cardIds: z.array(z.string()),
  controlMode: z.string(),
  controlDomain: z.number().nullable(),
  reviewedCards: z.array(reviewedCardSchema)
}) satisfies z.ZodType<PendingSessionResponse>

export const groupQuestionSchema = z.object({
  id: z.string(),
  domain: z.number(),
  scenario: z.string(),
  question: z.string(),
  options: z.record(z.string(), z.string())
}) satisfies z.ZodType<GroupQuestion>

export const groupVoteCountSchema = z.object({
  answer: z.string(),
  count: z.number()
}) satisfies z.ZodType<GroupVoteCount>

export const groupRoomStateSchema = z.object({
  code: z.string(),
  cert: z.string(),
  status: z.string(),
  currentIndex: z.number(),
  totalQuestions: z.number(),
  currentQuestion: groupQuestionSchema.nullable(),
  voteCounts: z.array(groupVoteCountSchema),
  totalVotes: z.number(),
  selectedAnswer: z.string().nullable(),
  correctAnswer: z.string().nullable(),
  explanation: z.string().nullable()
}) satisfies z.ZodType<GroupRoomState>

export const createGroupRoomResponseSchema = z.object({
  code: z.string(),
  hostToken: z.string(),
  joinUrl: z.string(),
  state: groupRoomStateSchema
}) satisfies z.ZodType<CreateGroupRoomResponse>
