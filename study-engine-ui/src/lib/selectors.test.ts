import { describe, expect, test } from 'vitest'
import { allTags, filterQuestions } from './browseSelectors'
import { barClass, browseCardBadge, percentage, studyCardLabel, tagLabel } from './presentation'
import {
  correctCount,
  domainBreakdown,
  missedCards,
  recentStreak,
  sessionAccuracy,
  streakMessage
} from './sessionSelectors'
import type { Question, QuestionEntry, SessionResult } from './types'

const questions: QuestionEntry[] = [
  {
    question: {
      id: 'q1',
      domain: 1,
      scenario: 'Agent routing',
      question: 'Which policy is explicit?',
      options: {},
      answer: 'A',
      explanation: '',
      tags: ['agents', 'routing']
    },
    cardState: { due: '2026-06-01', reps: 1 }
  },
  {
    question: {
      id: 'q2',
      domain: 2,
      scenario: 'Tool contract',
      question: 'What keeps calls safe?',
      options: {},
      answer: 'B',
      explanation: '',
      tags: ['tools']
    },
    cardState: null
  }
]

const results: SessionResult[] = [
  { cardId: 'q1', isCorrect: true, rating: 4, domain: 1 },
  { cardId: 'q2', isCorrect: false, rating: 1, domain: 1 },
  { cardId: 'q3', isCorrect: true, rating: 3, domain: 2 }
]

describe('presentation selectors', () => {
  test('classifies progress and labels tags', () => {
    expect(percentage(2, 3)).toBe(67)
    expect(percentage(1, 0)).toBe(0)
    expect(barClass(90)).toBe('strong')
    expect(barClass(60)).toBe('mid')
    expect(barClass(10)).toBe('low')
    expect(tagLabel(90)).toBe('✓ strong')
    expect(tagLabel(70)).toBe('~ ok')
    expect(tagLabel(30)).toBe('▼ needs work')
  })

  test('presents card scheduling badges and labels', () => {
    expect(browseCardBadge(null, '2026-06-05')).toEqual({ text: 'new', cls: 'badge-new' })
    expect(browseCardBadge({ due: '2026-06-04', reps: 1 }, '2026-06-05')).toEqual({
      text: 'due',
      cls: 'badge-due'
    })
    expect(browseCardBadge({ due: '2026-06-06', reps: 2 }, '2026-06-05')).toEqual({
      text: 'rep 2',
      cls: 'badge-ok'
    })
    expect(studyCardLabel({ due: '2026-06-06', reps: 1 })).toBe('reps=1  due=2026-06-06')
    expect(studyCardLabel({ due: '2026-06-06', reps: 3 })).toBe('reps=3')
  })
})

describe('browse selectors', () => {
  test('filters by domain, tag, and search', () => {
    expect(filterQuestions(questions, { domain: '1', tag: '', search: '' })).toHaveLength(1)
    expect(filterQuestions(questions, { domain: '', tag: 'tools', search: '' })[0].question.id).toBe('q2')
    expect(filterQuestions(questions, { domain: '', tag: '', search: 'routing' })[0].question.id).toBe('q1')
    expect(filterQuestions(questions, { domain: '', tag: '', search: 'missing' })).toHaveLength(0)
  })

  test('derives sorted unique tags', () => {
    expect(allTags(questions)).toEqual(['agents', 'routing', 'tools'])
  })

  test('treats a question with no tags field as untagged when tag-filtering', () => {
    const untagged: QuestionEntry[] = [{
      question: {
        id: 'q9', domain: 1, scenario: 's', question: 'q', options: {}, answer: 'A', explanation: ''
      } as Question,
      cardState: null
    }]
    expect(filterQuestions(untagged, { domain: '', tag: 'agents', search: '' })).toHaveLength(0)
  })
})

describe('session selectors', () => {
  test('summarizes score, streaks, domains, and misses', () => {
    expect(correctCount(results)).toBe(2)
    expect(sessionAccuracy(results)).toBe(67)
    expect(recentStreak(results, true)).toBe(2)
    expect(streakMessage(3)).toBe('3 in a row')
    expect(streakMessage(5)).toBe('5 straight')
    expect(streakMessage(7)).toBe('7 straight')
    expect(streakMessage(2)).toBe('')
    expect(domainBreakdown(results)).toEqual([
      { domain: '1', correct: 1, total: 2, pct: 50 },
      { domain: '2', correct: 1, total: 1, pct: 100 }
    ])
    expect(missedCards(results)).toEqual([results[1]])
  })

  test('omits results with no domain from the breakdown', () => {
    const mixed: SessionResult[] = [
      { cardId: 'a', isCorrect: true, rating: 4, domain: 1 },
      { cardId: 'b', isCorrect: false, rating: 1 } // no domain → skipped
    ]
    expect(domainBreakdown(mixed)).toEqual([
      { domain: '1', correct: 1, total: 1, pct: 100 }
    ])
  })
})
