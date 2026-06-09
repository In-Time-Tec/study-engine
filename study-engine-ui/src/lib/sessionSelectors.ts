import { percentage } from './presentation'
import type { SessionResult } from './types'

export interface DomainBreakdown {
  domain: string
  correct: number
  total: number
  pct: number
}

export function correctCount(results: SessionResult[]): number {
  return results.filter(result => result.isCorrect).length
}

export function sessionAccuracy(results: SessionResult[]): number {
  return percentage(correctCount(results), results.length)
}

export function recentStreak(results: SessionResult[], currentCorrect: boolean): number {
  let count = currentCorrect ? 1 : 0
  for (let i = results.length - 1; i >= 0; i--) {
    if (results[i].isCorrect) count++
    else break
  }
  return count
}

export function streakMessage(streak: number): string {
  if (streak === 3) return '3 in a row'
  if (streak === 5) return '5 straight'
  if (streak >= 7) return `${streak} straight`
  return ''
}

export function domainBreakdown(results: SessionResult[]): DomainBreakdown[] {
  const byDomain = results.reduce<Record<string, { correct: number; total: number }>>(
    (acc, result) => {
      if (result.domain == null) return acc
      const domain = String(result.domain)
      const current = acc[domain] ?? { correct: 0, total: 0 }
      acc[domain] = {
        correct: current.correct + (result.isCorrect ? 1 : 0),
        total: current.total + 1
      }
      return acc
    },
    {}
  )

  return Object.entries(byDomain).map(([domain, value]) => ({
    domain,
    correct: value.correct,
    total: value.total,
    pct: percentage(value.correct, value.total)
  }))
}

export function missedCards(results: SessionResult[]): SessionResult[] {
  return results.filter(result => !result.isCorrect)
}
