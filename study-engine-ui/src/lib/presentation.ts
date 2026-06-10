import type { CardState } from './types'

export type ProgressBand = 'strong' | 'mid' | 'low'

export interface BadgePresentation {
  text: string
  cls: 'badge-new' | 'badge-due' | 'badge-ok'
}

export function percentage(correct: number, total: number): number {
  return total > 0 ? Math.round(correct * 100 / total) : 0
}

export function barClass(pct: number): ProgressBand {
  if (pct >= 80) return 'strong'
  if (pct >= 50) return 'mid'
  return 'low'
}

export function tagLabel(pct: number): string {
  if (pct >= 85) return '✓ strong'
  if (pct >= 60) return '~ ok'
  return '▼ needs work'
}

export interface NextSessionStats {
  dueToday: number
  nextDue: string | null
  newAvailable: number
}

export function formatScheduleDate(date: string): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date)
  if (!match) return date

  const [, year, month, day] = match
  return new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric'
  }).format(new Date(Number(year), Number(month) - 1, Number(day)))
}

export function nextSessionDueText(
  stats: NextSessionStats | null,
  loading = false,
  error: string | null = null,
  hasBank = true
): string {
  if (loading) return 'Checking'
  if (!hasBank) return 'No bank'
  if (error) return 'Unavailable'
  if (!stats) return 'Checking'
  if (stats.dueToday > 0) return stats.dueToday === 1 ? 'Due today (1)' : `Due today (${stats.dueToday})`
  if (stats.nextDue) return `Due ${formatScheduleDate(stats.nextDue)}`
  if (stats.newAvailable > 0) return 'New cards ready'
  return 'All caught up'
}

export function localDateString(date = new Date()): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  return `${year}-${month}-${day}`
}

export function browseCardBadge(
  cardState: CardState | null,
  today = localDateString()
): BadgePresentation {
  if (!cardState || cardState.due === null) return { text: 'new', cls: 'badge-new' }
  if (cardState.due <= today) return { text: 'due', cls: 'badge-due' }
  return { text: `rep ${cardState.reps}`, cls: 'badge-ok' }
}

export function studyCardLabel(cardState: CardState | null): string {
  if (!cardState || cardState.due === null) return 'new'
  if (cardState.reps >= 3) return `reps=${cardState.reps}`
  return `reps=${cardState.reps}  due=${cardState.due}`
}
