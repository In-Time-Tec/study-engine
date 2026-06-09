import type { QuestionEntry } from './types'

export interface BrowseFilters {
  domain: string
  tag: string
  search: string
}

export function filterQuestions(
  questions: QuestionEntry[],
  { domain, tag, search }: BrowseFilters
): QuestionEntry[] {
  const normalizedSearch = search.trim().toLowerCase()
  const parsedDomain = domain ? parseInt(domain) : null

  return questions.filter(({ question }) => {
    if (parsedDomain !== null && question.domain !== parsedDomain) return false
    if (tag && !(question.tags || []).includes(tag)) return false
    if (!normalizedSearch) return true

    return (
      question.question.toLowerCase().includes(normalizedSearch) ||
      question.scenario.toLowerCase().includes(normalizedSearch)
    )
  })
}

export function allTags(questions: QuestionEntry[]): string[] {
  return [...new Set(questions.flatMap(({ question }) => question.tags || []))].sort()
}
