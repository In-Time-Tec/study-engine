import type {
  AnyCard,
  FetchDueOptions,
  QuestionOptions,
  SessionResult,
  StudyMode,
  StudyPhase,
  WrappedCard
} from './types'

const LETTERS = ['A', 'B', 'C', 'D']

export function isWrappedCard(card: AnyCard): card is WrappedCard {
  return 'cardState' in card
}

export interface StudyFetchParams {
  cert: string
  mode: StudyMode
  domain: number | null
  tag: string | null
  questionIds: string[] | null
}

/** Maps a study mode + filters to the fetchDue request options. */
export function studyFetchOptions(params: StudyFetchParams): FetchDueOptions {
  const { cert, mode, domain, tag, questionIds } = params
  if (mode === 'all') return { cert, all: true }
  if (mode === 'group') return { cert, all: true }
  if (mode === 'custom' && questionIds) return { cert, ids: questionIds, maxNew: 999 }
  return { cert, domain, tag, maxNew: 5 }
}

export type StudyIntent =
  | { kind: 'select'; letter: string }
  | { kind: 'rate'; rating: number }
  | null

/**
 * Maps a keypress to a study action given the current phase. Returns null when
 * the key has no effect. Pure: the component handles preventDefault and dispatch.
 */
export function keyIntent(
  phase: StudyPhase,
  key: string,
  options: QuestionOptions | null | undefined,
  isCorrect: boolean
): StudyIntent {
  if (phase === 'question') {
    const idx = parseInt(key) - 1
    if (idx >= 0 && idx < LETTERS.length) {
      const letter = LETTERS[idx]
      if (options && options[letter as keyof QuestionOptions]) {
        return { kind: 'select', letter }
      }
    }
    return null
  }
  if (phase === 'revealed') {
    if (key === '1') return { kind: 'rate', rating: 1 }
    if (key === '2' && isCorrect) return { kind: 'rate', rating: 3 }
    if (key === '3' && isCorrect) return { kind: 'rate', rating: 4 }
    if (key === ' ' || key === 'Enter') return { kind: 'rate', rating: isCorrect ? 3 : 1 }
    return null
  }
  return null
}

export function questionIdForCard(card: AnyCard | null | undefined): string | undefined {
  if (!card) return undefined
  return isWrappedCard(card) ? card.question.id : card.id
}

export function correctAnswerForCard(card: AnyCard | null | undefined): string | undefined {
  if (!card) return undefined
  return isWrappedCard(card) ? card.question.answer : card.answer
}

interface ApplyRatingInput {
  cards: AnyCard[]
  current: number
  selected: string | null
  sessionResults: SessionResult[]
}

interface ApplyRatingOutput {
  result: SessionResult
  sessionResults: SessionResult[]
  current: number
  selected: null
  phase: Extract<StudyPhase, 'question' | 'summary'>
  shouldFinish: boolean
}

export function applyRating(state: ApplyRatingInput, rating: number): ApplyRatingOutput {
  const { cards, current, selected, sessionResults } = state
  const card = cards[current]
  if (!card) {
    throw new Error('Cannot rate without a current card')
  }

  const q = isWrappedCard(card) ? card.question : card
  const result: SessionResult = {
    cardId: questionIdForCard(card)!,
    isCorrect: selected === correctAnswerForCard(card),
    rating,
    selected: selected ?? null,
    domain: q.domain,
    correctAnswer: q.answer,
    questionText: q.question,
  }

  const hasNextCard = current < cards.length - 1

  return {
    result,
    sessionResults: [...sessionResults, result],
    current: hasNextCard ? current + 1 : current,
    selected: null,
    phase: hasNextCard ? 'question' : 'summary',
    shouldFinish: !hasNextCard
  }
}
