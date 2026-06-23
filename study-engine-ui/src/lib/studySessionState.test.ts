import { describe, expect, test } from 'vitest'
import {
  applyRating,
  correctAnswerForCard,
  keyIntent,
  questionIdForCard,
  studyFetchOptions
} from './studySessionState'
import type { QuestionOptions, WrappedCard, Question } from './types'

const cards: WrappedCard[] = [
  {
    question: {
      id: 'q1',
      answer: 'B',
      domain: 1,
      scenario: '',
      question: '',
      options: {},
      explanation: '',
      tags: []
    },
    cardState: null
  },
  {
    question: {
      id: 'q2',
      answer: 'D',
      domain: 1,
      scenario: '',
      question: '',
      options: {},
      explanation: '',
      tags: []
    },
    cardState: null
  }
]

describe('study session state helpers', () => {
  test('rating a revealed card records the result and advances to the next question', () => {
    const next = applyRating({
      cards,
      current: 0,
      selected: 'B',
      sessionResults: []
    }, 4)

    expect(next.result).toEqual({
      cardId: 'q1',
      isCorrect: true,
      rating: 4,
      selected: 'B',
      domain: 1,
      correctAnswer: 'B',
      questionText: ''
    })
    expect(next.sessionResults).toEqual([next.result])
    expect(next.current).toBe(1)
    expect(next.selected).toBeNull()
    expect(next.phase).toBe('question')
    expect(next.shouldFinish).toBe(false)
  })

  test('rating the final card moves the session to summary', () => {
    const priorResult = { cardId: 'q1', isCorrect: true, rating: 4 }
    const next = applyRating({
      cards,
      current: 1,
      selected: 'A',
      sessionResults: [priorResult]
    }, 1)

    expect(next.result).toEqual({
      cardId: 'q2',
      isCorrect: false,
      rating: 1,
      selected: 'A',
      domain: 1,
      correctAnswer: 'D',
      questionText: ''
    })
    expect(next.sessionResults).toEqual([priorResult, next.result])
    expect(next.current).toBe(1)
    expect(next.selected).toBeNull()
    expect(next.phase).toBe('summary')
    expect(next.shouldFinish).toBe(true)
  })

  test('card helpers support wrapped API cards, plain cards, and missing cards', () => {
    expect(questionIdForCard(cards[0])).toBe('q1')
    expect(correctAnswerForCard(cards[0])).toBe('B')

    const plain: Question = {
      id: 'plain-card',
      answer: 'C',
      domain: 1,
      scenario: '',
      question: '',
      options: {},
      explanation: '',
      tags: []
    }
    expect(questionIdForCard(plain)).toBe('plain-card')
    expect(correctAnswerForCard(plain)).toBe('C')
    expect(questionIdForCard(null)).toBeUndefined()
    expect(correctAnswerForCard(null)).toBeUndefined()
  })

  test('rating with null selected records selected as null', () => {
    const next = applyRating({
      cards,
      current: 0,
      selected: null,
      sessionResults: []
    }, 1)

    expect(next.result.selected).toBeNull()
    expect(next.result.isCorrect).toBe(false)
  })

  test('rating without a current card is rejected', () => {
    expect(() => applyRating({
      cards: [],
      current: 0,
      selected: 'A',
      sessionResults: []
    }, 3)).toThrow('Cannot rate without a current card')
  })
})

describe('studyFetchOptions', () => {
  const base = { cert: 'cca-f', domain: null, tag: null, questionIds: null }

  test('all mode requests every card', () => {
    expect(studyFetchOptions({ ...base, mode: 'all' })).toEqual({ cert: 'cca-f', all: true })
  })

  test('group mode uses the all-card pool', () => {
    expect(studyFetchOptions({ ...base, mode: 'group' })).toEqual({ cert: 'cca-f', all: true })
  })

  test('custom mode with ids requests exactly those cards', () => {
    expect(studyFetchOptions({ ...base, mode: 'custom', questionIds: ['q1', 'q2'] }))
      .toEqual({ cert: 'cca-f', ids: ['q1', 'q2'], maxNew: 999 })
  })

  test('custom mode without ids falls back to the default due query', () => {
    expect(studyFetchOptions({ ...base, mode: 'custom', questionIds: null }))
      .toEqual({ cert: 'cca-f', domain: null, tag: null, maxNew: 5 })
  })

  test('due mode passes domain and tag filters with the new-card cap', () => {
    expect(studyFetchOptions({ cert: 'cca-f', mode: 'due', domain: 2, tag: 'tools', questionIds: null }))
      .toEqual({ cert: 'cca-f', domain: 2, tag: 'tools', maxNew: 5 })
  })
})

describe('keyIntent', () => {
  const options: QuestionOptions = { A: 'a', B: 'b', C: 'c' }

  test('question phase selects the matching option when it exists', () => {
    expect(keyIntent('question', '2', options, false)).toEqual({ kind: 'select', letter: 'B' })
  })

  test('question phase ignores a number with no corresponding option', () => {
    expect(keyIntent('question', '4', options, false)).toBeNull() // D not present
  })

  test('question phase ignores out-of-range and non-numeric keys', () => {
    expect(keyIntent('question', '0', options, false)).toBeNull()  // idx -1 < 0
    expect(keyIntent('question', '9', options, false)).toBeNull()  // idx 8 >= 4
    expect(keyIntent('question', 'x', options, false)).toBeNull()  // NaN
  })

  test('question phase ignores keys when options are absent', () => {
    expect(keyIntent('question', '1', null, false)).toBeNull()
    expect(keyIntent('question', '1', undefined, false)).toBeNull()
  })

  test('revealed phase: 1 always rates Again', () => {
    expect(keyIntent('revealed', '1', options, true)).toEqual({ kind: 'rate', rating: 1 })
    expect(keyIntent('revealed', '1', options, false)).toEqual({ kind: 'rate', rating: 1 })
  })

  test('revealed phase: 2 and 3 rate Good/Easy only when correct', () => {
    expect(keyIntent('revealed', '2', options, true)).toEqual({ kind: 'rate', rating: 3 })
    expect(keyIntent('revealed', '3', options, true)).toEqual({ kind: 'rate', rating: 4 })
    expect(keyIntent('revealed', '2', options, false)).toBeNull()
    expect(keyIntent('revealed', '3', options, false)).toBeNull()
  })

  test('revealed phase: Space/Enter use the smart default rating', () => {
    expect(keyIntent('revealed', ' ', options, true)).toEqual({ kind: 'rate', rating: 3 })
    expect(keyIntent('revealed', 'Enter', options, false)).toEqual({ kind: 'rate', rating: 1 })
  })

  test('revealed phase: unrelated keys do nothing', () => {
    expect(keyIntent('revealed', 'q', options, true)).toBeNull()
  })

  test('other phases never produce an intent', () => {
    expect(keyIntent('summary', '1', options, true)).toBeNull()
    expect(keyIntent('loading', ' ', options, false)).toBeNull()
  })
})
