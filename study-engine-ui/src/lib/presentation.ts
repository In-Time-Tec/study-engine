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
