import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import App from '../App.svelte'
import Browse from './Browse.svelte'
import Dashboard from './Dashboard.svelte'
import History from './History.svelte'
import StudySession from './StudySession.svelte'
import type { Stats, DueResponse, QuestionsResponse, SessionsResponse, WrappedCard, Question } from './types'

const api = vi.hoisted(() => ({
  fetchCerts: vi.fn(),
  fetchDue: vi.fn(),
  fetchQuestions: vi.fn(),
  fetchSessions: vi.fn(),
  fetchStats: vi.fn(),
  postReview: vi.fn(),
  postSession: vi.fn(),
  fetchBanks: vi.fn(),
  uploadBank: vi.fn(),
  deleteBank: vi.fn()
}))

vi.mock('./api', () => api)

const statsFull: Stats = {
  dueToday: 3,
  newAvailable: 7,
  mastered: 9,
  total: 20,
  domains: [
    { id: 1, name: 'Architecture', accuracy: 90, mastered: 5, total: 6, reviewTotal: 3 },
    { id: 2, name: 'Tools', accuracy: 70, mastered: 3, total: 5, reviewTotal: 2 },
    { id: 3, name: 'Context', accuracy: 40, mastered: 1, total: 4, reviewTotal: 1 }
  ],
  tags: [
    { tag: 'agents', accuracy: 90 },
    { tag: 'tools', accuracy: 70 },
    { tag: 'context', accuracy: 40 }
  ],
  sessions: [
    { date: '2026-06-01', correct: 8, total: 10, accuracy: 80 },
    { date: '2026-06-02', correct: 5, total: 10, accuracy: 50 },
    { date: '2026-06-03', correct: 4, total: 10, accuracy: 40 }
  ]
}

const statsEmptyProgress: Stats = {
  dueToday: 0,
  newAvailable: 4,
  mastered: 0,
  total: 4,
  domains: [
    { id: 1, name: 'Architecture', accuracy: 0, mastered: 0, total: 4, reviewTotal: 0 }
  ],
  tags: [],
  sessions: []
}

const questionOneLong = 'Which architecture pattern best coordinates tool-calling agents when domain ownership, escalation rules, and memory boundaries must remain clear across a long-running certification workflow?'

const questionsData: QuestionsResponse = {
  domains: {
    1: 'Architecture',
    2: 'Tooling'
  },
  questions: [
    {
      question: {
        id: 'q1',
        domain: 1,
        scenario: 'Coordinator handoff',
        question: questionOneLong,
        options: { A: 'Shared mutable memory', B: 'Explicit routing policy', C: 'Hidden global state' },
        answer: 'B',
        explanation: 'Routing policies keep ownership visible.',
        tags: ['agents', 'routing']
      },
      cardState: { due: '2000-01-01', reps: 1 }
    },
    {
      question: {
        id: 'q2',
        domain: 2,
        scenario: 'MCP inventory',
        question: 'Which tool contract matters most?',
        options: { A: 'Typed inputs', B: 'Pretty labels', C: 'Long names', D: 'Extra colors' },
        answer: 'A',
        explanation: 'Typed inputs make tool use reliable.',
        tags: ['tools']
      },
      cardState: { due: '2999-01-01', reps: 2 }
    },
    {
      question: {
        id: 'q3',
        domain: 2,
        scenario: 'Search path',
        question: 'How should context be retrieved?',
        options: { A: 'Guess', B: 'Search first' },
        answer: 'B',
        explanation: 'Search narrows the context.',
        tags: []
      },
      cardState: { due: null, reps: 0 }
    },
    {
      question: {
        id: 'q4',
        domain: 1,
        scenario: 'No card state',
        question: 'What status should a missing card state show?',
        options: { A: 'new' },
        answer: 'A',
        explanation: 'Missing scheduling data is new.'
      } as Question,
      cardState: null
    }
  ]
}

const wrappedCard: WrappedCard = {
  question: {
    id: 'q1',
    domain: 1,
    scenario: 'Coordinator handoff',
    question: 'What should you choose first?',
    options: { A: 'Guess', B: 'Route explicitly', C: 'Skip', D: 'Restart' },
    answer: 'B',
    explanation: 'Explicit routing keeps the workflow inspectable.',
    tags: ['agents']
  },
  cardState: { due: '2026-06-04', reps: 2 }
}

const plainCard: Question = {
  id: 'q2',
  domain: 2,
  scenario: 'MCP contract',
  question: 'Which input shape is safest?',
  options: { A: 'Typed schema', B: 'Loose string', C: 'Anything', D: 'None' },
  answer: 'A',
  explanation: 'Typed schemas constrain the tool call.',
  tags: []
}

const masteredCard: WrappedCard = {
  question: {
    id: 'q3',
    domain: 3,
    scenario: 'Review cadence',
    question: 'How many reps show mastery?',
    options: { A: 'One', B: 'Two', C: 'Three', D: 'Four' },
    answer: 'C',
    explanation: 'Three reps marks this card as practiced.',
    tags: []
  },
  cardState: { due: '2999-01-01', reps: 3 }
}

function resetApi(): void {
  for (const fn of Object.values(api)) {
    fn.mockReset()
  }
  api.fetchCerts.mockResolvedValue(['cca-f'])
  api.postReview.mockResolvedValue({})
  api.postSession.mockResolvedValue({})
  api.fetchBanks.mockResolvedValue([])
}

describe('Dashboard', () => {
  beforeEach(resetApi)

  test('renders loaded stats, progress bands, sessions, and clickable domain bars', async () => {
    api.fetchStats.mockResolvedValue(statsFull)
    const onstudy = vi.fn()
    const { container } = render(Dashboard, { props: { onstudy } })

    expect(screen.getByText(/loading/)).toBeInTheDocument()
    expect(await screen.findByText('Due Today', { selector: '.stat-label' })).toBeInTheDocument()
    expect(screen.getByText('D1 Architecture')).toBeInTheDocument()
    expect(screen.getByText('#agents')).toBeInTheDocument()
    expect(screen.getByText('✓ strong')).toBeInTheDocument()
    expect(screen.getByText('~ ok')).toBeInTheDocument()
    expect(screen.getByText('▼ needs work')).toBeInTheDocument()
    expect(screen.getByText('2026-06-01')).toBeInTheDocument()
    expect(container.querySelectorAll('.bar-fill.strong')).toHaveLength(3)
    expect(container.querySelectorAll('.bar-fill.mid')).toHaveLength(3)
    expect(container.querySelectorAll('.bar-fill.low')).toHaveLength(3)

    // A domain bar is a button: clicking it launches a due session scoped to it.
    await fireEvent.click(screen.getByText('D1 Architecture').closest('button')!)
    expect(onstudy).toHaveBeenCalledWith({ mode: 'due', domain: 1 })
  })

  test('clicking a different domain bar passes that domain in the study event', async () => {
    api.fetchStats.mockResolvedValue(statsFull)
    const onstudy = vi.fn()
    render(Dashboard, { props: { onstudy } })

    await screen.findByText('Due Today', { selector: '.stat-label' })
    await fireEvent.click(screen.getByText('D2 Tools').closest('button')!)
    expect(onstudy).toHaveBeenCalledWith({ mode: 'due', domain: 2 })
  })

  test('shows empty progress copy when no domains have reviews', async () => {
    api.fetchStats.mockResolvedValue(statsEmptyProgress)
    render(Dashboard)

    expect(await screen.findByText('No reviews yet. Start a session to see progress.')).toBeInTheDocument()
    expect(screen.queryByText('Concept Mastery', { selector: '.panel-title' })).not.toBeInTheDocument()
    expect(screen.queryByText('Recent Sessions', { selector: '.panel-title' })).not.toBeInTheDocument()
  })

  test('shows API errors', async () => {
    api.fetchStats.mockRejectedValue(new Error('stats failed'))
    render(Dashboard)

    expect(await screen.findByText('Error loading stats: stats failed')).toBeInTheDocument()
  })
})

describe('Browse', () => {
  beforeEach(resetApi)

  test('filters, expands, collapses, badges, and quizzes the current result set', async () => {
    api.fetchQuestions.mockResolvedValue(questionsData)
    const onquiz = vi.fn()
    const { container } = render(Browse, { props: { onquiz } })

    expect(screen.getByText(/loading/)).toBeInTheDocument()
    expect(await screen.findByText('4 of 4 questions')).toBeInTheDocument()
    expect(screen.getByText('due')).toHaveClass('badge-due')
    expect(screen.getByText('rep 2')).toHaveClass('badge-ok')
    expect(screen.getAllByText('new')).toHaveLength(2)
    expect(screen.getByText(`${questionOneLong.slice(0, 120)}…`)).toBeInTheDocument()

    await fireEvent.click(screen.getByText('Coordinator handoff').closest('.q-row-header')!)
    expect(screen.getByText(questionOneLong)).toBeInTheDocument()
    expect(screen.getByText('Answer: B')).toBeInTheDocument()
    expect(screen.getAllByText('#routing')).toHaveLength(2)
    expect(container.querySelector('.reveal-correct')).toHaveTextContent('B')

    await fireEvent.click(screen.getByText('Coordinator handoff').closest('.q-row-header')!)
    expect(screen.queryByText('Answer: B')).not.toBeInTheDocument()

    await fireEvent.keyDown(screen.getByText('MCP inventory').closest('.q-row-header')!, { key: 'Enter' })
    expect(screen.getByText('Typed inputs make tool use reliable.')).toBeInTheDocument()

    await fireEvent.change(screen.getAllByRole('combobox')[0], { target: { value: '2' } })
    expect(screen.getByText('2 of 4 questions')).toBeInTheDocument()

    await fireEvent.change(screen.getAllByRole('combobox')[1], { target: { value: 'tools' } })
    expect(screen.getByText('1 of 4 questions')).toBeInTheDocument()

    await fireEvent.input(screen.getByPlaceholderText(/Search question or scenario/), { target: { value: 'MCP' } })
    expect(screen.getByText('1 of 4 questions')).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Quiz 1 cards' }))
    expect(onquiz).toHaveBeenCalledWith({ questionIds: ['q2'] })

    await fireEvent.input(screen.getByPlaceholderText(/Search question or scenario/), { target: { value: 'absent' } })
    expect(screen.getByText('No questions match this filter.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Quiz 0 cards' })).toBeDisabled()
  })

  test('non-Enter keydown does not toggle expansion', async () => {
    api.fetchQuestions.mockResolvedValue(questionsData)
    render(Browse)
    await screen.findByText('4 of 4 questions')

    await fireEvent.keyDown(screen.getByText('Search path').closest('.q-row-header')!, { key: 'Space' })
    expect(screen.queryByText('Search narrows the context.')).not.toBeInTheDocument()
  })

  test('expanded question with empty tags shows no tag section', async () => {
    api.fetchQuestions.mockResolvedValue(questionsData)
    render(Browse)
    await screen.findByText('4 of 4 questions')

    await fireEvent.click(screen.getByText('Search path').closest('.q-row-header')!)
    expect(screen.getByText('Search narrows the context.')).toBeInTheDocument()
    expect(screen.queryByText(/^#(?!)/)).not.toBeInTheDocument()
    const detailTags = document.querySelectorAll('.q-detail .tag')
    expect(detailTags).toHaveLength(0)
  })

  test('shows API errors', async () => {
    api.fetchQuestions.mockRejectedValue(new Error('questions failed'))
    render(Browse)

    expect(await screen.findByText('Error: questions failed')).toBeInTheDocument()
  })
})

describe('StudySession', () => {
  beforeEach(resetApi)

  test('loads due cards, records reviews, posts the completed session, and reloads from summary', async () => {
    api.fetchDue
      .mockResolvedValueOnce({ cards: [wrappedCard, plainCard], dueCount: 1, newCount: 1 } satisfies DueResponse)
      .mockResolvedValueOnce({ cards: [], dueCount: 0, newCount: 0 } satisfies DueResponse)

    render(StudySession)

    expect(screen.getByText(/loading session/)).toBeInTheDocument()
    await screen.findByText('What should you choose first?')
    expect(document.querySelector('.session-progress')).toHaveTextContent('Card 1 / 2')
    expect(document.querySelector('.session-progress')).toHaveTextContent('1 due + 1 new')
    expect(screen.getByText(/reps=2\s+due=2026-06-04/)).toHaveClass('badge-due')

    await fireEvent.click(screen.getByRole('button', { name: /A Guess/ }))
    expect(screen.getByText('✗ Incorrect — correct answer: B')).toHaveClass('result-wrong')
    expect(screen.getByText('Explicit routing keeps the workflow inspectable.')).toBeInTheDocument()
    expect(screen.getByText('#agents')).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Again' }))
    await waitFor(() => expect(api.postReview).toHaveBeenCalledWith({
      cardId: 'q1',
      cert: 'cca-f',
      rating: 1,
      isCorrect: false
    }))
    await screen.findByText('Which input shape is safest?')
    expect(document.querySelector('.session-progress')).toHaveTextContent('Card 2 / 2')
    expect(screen.getByText('new')).toHaveClass('badge-new')

    await fireEvent.click(screen.getByRole('button', { name: /A Typed schema/ }))
    expect(screen.getByText('✓ Correct')).toHaveClass('result-correct')
    await fireEvent.click(screen.getByRole('button', { name: 'Easy' }))

    expect(await screen.findByText('50%')).toBeInTheDocument()
    expect(screen.getByText('1 / 2 correct')).toBeInTheDocument()
    await waitFor(() => expect(api.postSession).toHaveBeenCalledWith({ cert: 'cca-f', total: 2, correct: 1 }))

    await fireEvent.click(screen.getByRole('button', { name: 'Study Again' }))
    expect(await screen.findByText('Nothing due today.')).toBeInTheDocument()
    expect(api.fetchDue).toHaveBeenNthCalledWith(1, { cert: 'cca-f', domain: null, tag: null, maxNew: 5 })
  })

  test('loads all, custom, and filtered sessions with the right API options', async () => {
    api.fetchDue.mockResolvedValue({ cards: [], dueCount: 0, newCount: 0 } satisfies DueResponse)

    render(StudySession, { props: { mode: 'all' } })
    await screen.findByText('Nothing due today.')
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', all: true })

    render(StudySession, { props: { mode: 'custom', questionIds: ['q1', 'q2'] } })
    await screen.findAllByText('Nothing due today.')
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', ids: ['q1', 'q2'], maxNew: 999 })

    render(StudySession, { props: { domain: 2, tag: 'tools' } })
    await screen.findAllByText('Nothing due today.')
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', domain: 2, tag: 'tools', maxNew: 5 })
  })

  test('completes a non-due session with a good rating and no study-again action', async () => {
    api.fetchDue.mockResolvedValue({ cards: [masteredCard], dueCount: 0, newCount: 1 } satisfies DueResponse)
    const ondone = vi.fn()
    render(StudySession, { props: { mode: 'all', ondone } })

    expect(await screen.findByText('How many reps show mastery?')).toBeInTheDocument()
    expect(screen.getByText('1 new')).toBeInTheDocument()
    expect(screen.getByText('reps=3')).toHaveClass('badge-ok')

    await fireEvent.click(screen.getByRole('button', { name: /C Three/ }))
    expect(screen.queryByText('#agents')).not.toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Good' }))

    expect(await screen.findByText('100%')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Study Again' })).not.toBeInTheDocument()
    expect(api.postReview).toHaveBeenCalledWith({
      cardId: 'q3',
      cert: 'cca-f',
      rating: 3,
      isCorrect: true
    })

    await fireEvent.click(screen.getByRole('button', { name: 'Back to Dashboard' }))
    expect(ondone).toHaveBeenCalledOnce()
  })

  test('postReview rejection stays on the revealed card and shows a save error', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard], dueCount: 1, newCount: 0 } satisfies DueResponse)
    api.postReview.mockRejectedValue(new Error('network error'))

    render(StudySession)
    await screen.findByText('What should you choose first?')
    await fireEvent.click(screen.getByRole('button', { name: /B Route explicitly/ }))
    await fireEvent.click(screen.getByRole('button', { name: 'Good' }))

    expect(await screen.findByText('Save failed: network error')).toBeInTheDocument()
    expect(screen.getByText('What should you choose first?')).toBeInTheDocument()
    expect(screen.queryByText('100%')).not.toBeInTheDocument()
  })

  test('postSession rejection renders summary with a save error', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard], dueCount: 1, newCount: 0 } satisfies DueResponse)
    api.postSession.mockRejectedValue(new Error('save failed'))

    render(StudySession)
    await screen.findByText('What should you choose first?')
    await fireEvent.click(screen.getByRole('button', { name: /B Route explicitly/ }))
    await fireEvent.click(screen.getByRole('button', { name: 'Good' }))

    expect(await screen.findByText('100%')).toBeInTheDocument()
    expect(await screen.findByText('Save failed: save failed')).toBeInTheDocument()
  })

  test('clicking an answer in revealed phase is ignored', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard], dueCount: 1, newCount: 0 } satisfies DueResponse)
    render(StudySession)

    await screen.findByText('What should you choose first?')
    await fireEvent.click(screen.getByRole('button', { name: /B Route explicitly/ }))
    expect(screen.getByText('✓ Correct')).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: /A Guess/ }))
    expect(screen.getByText('✓ Correct')).toBeInTheDocument()
  })

  test('keyboard 1-4 selects options in question phase, 1/2/3 rates in revealed phase', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard, plainCard], dueCount: 1, newCount: 1 } satisfies DueResponse)
    render(StudySession)
    await screen.findByText('What should you choose first?')

    // key '2' selects option B in question phase
    await fireEvent.keyDown(document.body, { key: '2' })
    expect(screen.getByText('✓ Correct')).toBeInTheDocument()

    // key '2' in revealed phase with correct answer → rate Good (3)
    await fireEvent.keyDown(document.body, { key: '2' })
    await waitFor(() => expect(api.postReview).toHaveBeenCalledWith({
      cardId: 'q1', cert: 'cca-f', rating: 3, isCorrect: true
    }))
    await screen.findByText('Which input shape is safest?')

    // key '2' selects wrong option B (correct is A), then key '1' in revealed → rate Again (1)
    await fireEvent.keyDown(document.body, { key: '2' })
    expect(screen.getByText('✗ Incorrect — correct answer: A')).toBeInTheDocument()
    await fireEvent.keyDown(document.body, { key: '1' })
    await waitFor(() => expect(api.postReview).toHaveBeenCalledWith({
      cardId: 'q2', cert: 'cca-f', rating: 1, isCorrect: false
    }))
  })

  test('keyboard Space/Enter uses smart default rating; keys ignored outside question/revealed', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard], dueCount: 1, newCount: 0 } satisfies DueResponse)
    render(StudySession)
    await screen.findByText('What should you choose first?')

    // Enter in loading/question does nothing before answer selected
    await fireEvent.keyDown(document.body, { key: 'Enter' })
    expect(screen.queryByText('✓ Correct')).not.toBeInTheDocument()

    // select B (correct), then Space → smart default Good (3)
    await fireEvent.keyDown(document.body, { key: '2' })
    expect(screen.getByText('✓ Correct')).toBeInTheDocument()
    await fireEvent.keyDown(document.body, { key: ' ' })
    expect(await screen.findByText('100%')).toBeInTheDocument()

    // keys in summary phase do nothing
    await fireEvent.keyDown(document.body, { key: '1' })
    expect(screen.getByText('100%')).toBeInTheDocument()
  })

  test('streak message appears after 3 consecutive correct answers', async () => {
    const threeCards = [wrappedCard, masteredCard, plainCard]
    api.fetchDue.mockResolvedValue({ cards: threeCards, dueCount: 1, newCount: 2 } satisfies DueResponse)
    render(StudySession)
    await screen.findByText('What should you choose first?')

    // card 1 correct (B)
    await fireEvent.keyDown(document.body, { key: '2' })
    await fireEvent.keyDown(document.body, { key: '2' })
    await screen.findByText('How many reps show mastery?')

    // card 2 correct (C)
    await fireEvent.keyDown(document.body, { key: '3' })
    await fireEvent.keyDown(document.body, { key: '2' })
    await screen.findByText('Which input shape is safest?')

    // card 3 correct (A) → streak = 3 → message should appear
    await fireEvent.keyDown(document.body, { key: '1' })
    expect(screen.getByText('3 in a row')).toBeInTheDocument()
  })

  test('summary shows domain breakdown and missed cards', async () => {
    api.fetchDue.mockResolvedValue({ cards: [wrappedCard, plainCard], dueCount: 1, newCount: 1 } satisfies DueResponse)
    const { container } = render(StudySession)
    await screen.findByText('What should you choose first?')

    // answer card 1 wrong (select A, correct is B)
    await fireEvent.click(screen.getByRole('button', { name: /A Guess/ }))
    await fireEvent.click(screen.getByRole('button', { name: 'Again' }))

    // answer card 2 correct (select A, correct is A)
    await screen.findByText('Which input shape is safest?')
    await fireEvent.click(screen.getByRole('button', { name: /A Typed schema/ }))
    await fireEvent.click(screen.getByRole('button', { name: 'Easy' }))

    expect(await screen.findByText('50%')).toBeInTheDocument()
    expect(screen.getByText('1 / 2 correct')).toBeInTheDocument()

    // domain breakdown: D1 (wrappedCard) and D2 (plainCard)
    expect(screen.getByText('D1')).toBeInTheDocument()
    expect(screen.getByText('D2')).toBeInTheDocument()

    // missed cards section: wrappedCard was wrong
    const missedCell = document.querySelector('.missed-text')
    expect(missedCell).toHaveTextContent('What should you choose first')
    expect(document.querySelector('.missed-answer')).toHaveTextContent('you: A · correct: B')

    // back to dashboard button still works
    expect(screen.getByRole('button', { name: 'Back to Dashboard' })).toBeInTheDocument()
  })

  test('shows load errors and dispatches done from the empty state', async () => {
    api.fetchDue.mockRejectedValueOnce(new Error('due failed'))
    render(StudySession)
    expect(await screen.findByText('Error: due failed')).toBeInTheDocument()

    api.fetchDue.mockResolvedValueOnce({ cards: [], dueCount: 0, newCount: 0 } satisfies DueResponse)
    const ondone = vi.fn()
    render(StudySession, { props: { ondone } })

    expect(await screen.findByText('Nothing due today.')).toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Back to Dashboard' }))
    expect(ondone).toHaveBeenCalledOnce()
  })
})

describe('History', () => {
  beforeEach(resetApi)

  test('renders session history with high, medium, and low accuracy styling', async () => {
    api.fetchSessions.mockResolvedValue({
      sessions: [
        { date: '2026-06-01', correct: 9, total: 10, accuracy: 90 },
        { date: '2026-06-02', correct: 6, total: 10, accuracy: 60 },
        { date: '2026-06-03', correct: 3, total: 10, accuracy: 30 }
      ]
    } satisfies SessionsResponse)
    const { container } = render(History)

    expect(await screen.findByText('2026-06-01')).toBeInTheDocument()
    expect(screen.getByText('9 / 10')).toBeInTheDocument()
    expect(container.querySelectorAll('.bar-fill.strong')).toHaveLength(1)
    expect(container.querySelectorAll('.bar-fill.mid')).toHaveLength(1)
    expect(container.querySelectorAll('.bar-fill.low')).toHaveLength(1)
  })

  test('shows empty and error states', async () => {
    api.fetchSessions.mockResolvedValueOnce({} satisfies SessionsResponse)
    render(History)
    expect(await screen.findByText('No sessions recorded yet.')).toBeInTheDocument()

    api.fetchSessions.mockRejectedValueOnce(new Error('history failed'))
    render(History)
    expect(await screen.findByText('Error: history failed')).toBeInTheDocument()
  })
})

describe('App', () => {
  beforeEach(resetApi)

  test('coordinates navigation, dashboard study events, browse quiz events, history, and done', async () => {
    api.fetchStats.mockResolvedValue(statsFull)
    api.fetchDue.mockResolvedValue({ cards: [], dueCount: 0, newCount: 0 } satisfies DueResponse)
    api.fetchQuestions.mockResolvedValue(questionsData)
    api.fetchSessions.mockResolvedValue({ sessions: [{ date: '2026-06-01', correct: 1, total: 1, accuracy: 100 }] } satisfies SessionsResponse)

    render(App)

    expect(await screen.findByText('Due Today', { selector: '.stat-label' })).toBeInTheDocument()

    // The Study nav button launches the default due session directly
    await fireEvent.click(screen.getByRole('button', { name: 'Study' }))
    expect(await screen.findByText('Nothing due today.')).toBeInTheDocument()
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', domain: null, tag: null, maxNew: 5 })

    // The Study tab owns the All toggle that used to live on the Dashboard:
    // flipping it re-runs the session over the whole bank, ignoring scheduling.
    await fireEvent.click(screen.getByRole('button', { name: 'All' }))
    await waitFor(() => expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', all: true }))

    await fireEvent.click(screen.getByRole('button', { name: 'Back to Dashboard' }))
    expect(await screen.findByText('Due Today', { selector: '.stat-label' })).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Browse' }))
    expect(await screen.findByText('4 of 4 questions')).toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Quiz 4 cards' }))
    expect(await screen.findByText('Nothing due today.')).toBeInTheDocument()
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', ids: ['q1', 'q2', 'q3', 'q4'], maxNew: 999 })

    await fireEvent.click(screen.getByRole('button', { name: 'History' }))
    expect(await screen.findByText('2026-06-01')).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Dashboard' }))
    expect(await screen.findByText('Due Today', { selector: '.stat-label' })).toBeInTheDocument()
  })

  test('clicking a dashboard domain bar launches a domain-filtered study session', async () => {
    api.fetchStats.mockResolvedValue(statsFull)
    api.fetchDue.mockResolvedValue({ cards: [], dueCount: 0, newCount: 0 } satisfies DueResponse)

    render(App)
    await screen.findByText('Due Today', { selector: '.stat-label' })

    await fireEvent.click(screen.getByText('D2 Tools').closest('button')!)
    expect(await screen.findByText('Nothing due today.')).toBeInTheDocument()
    expect(api.fetchDue).toHaveBeenLastCalledWith({ cert: 'cca-f', domain: 2, tag: null, maxNew: 5 })
  })

  test('shows the empty-bank message and routes to Settings when no bank is available', async () => {
    api.fetchCerts.mockRejectedValueOnce(new Error('no banks on disk'))

    render(App)

    expect(await screen.findByText('No question bank yet.')).toBeInTheDocument()
    expect(screen.queryByText('Dashboard')).toBeInTheDocument() // nav still renders
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument() // no bank selector with zero certs

    // The inline Settings link (not the nav tab) opens the upload page even
    // with no banks loaded. [0] is the nav tab, [1] is the empty-state link.
    const settingsButtons = screen.getAllByRole('button', { name: 'Settings' })
    await fireEvent.click(settingsButtons[1])
    expect(await screen.findByText('Upload a Bank')).toBeInTheDocument()
  })

  test('renders a bank selector and switches the active bank when multiple certs exist', async () => {
    api.fetchCerts.mockResolvedValue(['cca-f', 'aws-saa'])
    api.fetchStats.mockResolvedValue(statsFull)

    render(App)
    await screen.findByText('Due Today', { selector: '.stat-label' })

    const bankSelect = screen.getByLabelText('Bank:') as HTMLSelectElement
    expect(bankSelect.value).toBe('cca-f')
    expect(screen.getByRole('option', { name: 'aws-saa' })).toBeInTheDocument()

    await fireEvent.change(bankSelect, { target: { value: 'aws-saa' } })
    expect(bankSelect.value).toBe('aws-saa')
  })
})
