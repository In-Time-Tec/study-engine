<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { fade, fly } from 'svelte/transition'
  import { cubicOut } from 'svelte/easing'
  import { clearPendingSession, fetchDue, fetchStats, loadPendingSession, postReview, postSession, savePendingSession } from './api'
  import GroupHost from './GroupHost.svelte'
  import { barClass, studyCardLabel } from './presentation'
  import {
    correctCount as countCorrect,
    domainBreakdown as summarizeDomains,
    missedCards as selectMissedCards,
    recentStreak as countRecentStreak,
    sessionAccuracy,
    streakMessage as messageForStreak
  } from './sessionSelectors'
  import { applyRating, correctAnswerForCard, isWrappedCard, keyIntent, studyFetchOptions } from './studySessionState'
  import type { AnyCard, CardState, DomainStat, Question, SessionResult, StudyMode, StudyPhase } from './types'

  let {
    cert = 'cca-f',
    mode = 'due' as StudyMode,
    domain = null as number | null,
    tag = null as string | null,
    questionIds = null as string[] | null,
    ondone
  }: {
    cert?: string
    mode?: StudyMode
    domain?: number | null
    tag?: string | null
    questionIds?: string[] | null
    ondone?: () => void
  } = $props()

  // loading → question → revealed → (next question | done) → summary
  let phase: StudyPhase = $state('loading')
  let cards: AnyCard[] = $state([])
  let current: number = $state(0)
  let selected: string | null = $state(null)
  let sessionResults: SessionResult[] = $state([])
  let error: string | null = $state(null)
  let saveError: string | null = $state(null)
  let saving: boolean = $state(false)
  let confirmingEnd: boolean = $state(false)

  let dueCount: number = $state(0)
  let newCount: number = $state(0)

  // The Study tab is self-sufficient: it owns its own mode and domain rather
  // than depending solely on the props it was launched with. The props seed the
  // initial values (e.g. a Dashboard domain-bar deep-link), then the in-session
  // toggle and selector drive reloads. Hidden in 'custom' mode, which studies an
  // explicit id list handed over from Browse.
  // untrack() reads the initial prop value without establishing a reactive dependency —
  // these are intentional one-time seeds, not derived values.
  const interactive: boolean = untrack(() => mode !== 'custom')
  let controlMode: Exclude<StudyMode, 'custom'> = $state(untrack(() => mode === 'all' ? 'all' : mode === 'group' ? 'group' : 'due'))
  let controlDomain: number | null = $state(untrack(() => domain))
  let domains: DomainStat[] = $state([])

  function getCardId(c: AnyCard): string {
    return isWrappedCard(c) ? c.question.id : (c as Question).id
  }

  async function loadSession(): Promise<void> {
    if (interactive && controlMode === 'group') {
      clearPendingSession(cert).catch(() => {})
      confirmingEnd = false
      phase = 'empty'
      return
    }
    clearPendingSession(cert).catch(() => {})
    confirmingEnd = false
    phase = 'loading'
    current = 0
    selected = null
    sessionResults = []
    saveError = null
    saving = false
    try {
      const data = await fetchDue(studyFetchOptions({
        cert,
        mode: interactive ? controlMode : mode,
        domain: interactive ? controlDomain : domain,
        tag,
        questionIds
      }))
      cards = data.cards
      dueCount = data.dueCount
      newCount = data.newCount
      phase = cards.length ? 'question' : 'empty'
      if (interactive && cards.length > 0) {
        savePendingSession({
          cert,
          cardIds: cards.map(getCardId),
          controlMode,
          controlDomain
        }).catch(() => {})
      }
    } catch (e) {
      error = (e as Error).message
      phase = 'error'
    }
  }

  onMount(async () => {
    // Populate the domain selector. Failure (or no stats) just leaves the
    // selector at "All domains" — it must never block the study session itself.
    if (interactive) {
      Promise.resolve(fetchStats(cert))
        .then(s => { domains = s?.domains ?? [] })
        .catch(() => { domains = [] })
    }

    if (interactive) {
      if (controlMode === 'group') {
        phase = 'empty'
        return
      }
      try {
        const pending = await loadPendingSession(cert)
        if (pending && pending.cardIds.length > 0) {
          const data = await fetchDue({ cert, ids: pending.cardIds })
          const cardMap = new Map(data.cards.map(c => [getCardId(c), c]))
          const orderedCards = pending.cardIds
            .map(id => cardMap.get(id))
            .filter((c): c is AnyCard => c !== undefined)
          const resumeIndex = pending.reviewedCards.length
          if (resumeIndex < orderedCards.length) {
            cards = orderedCards
            current = resumeIndex
            selected = null
            sessionResults = pending.reviewedCards.map(rc => ({
              cardId: rc.cardId,
              isCorrect: rc.isCorrect,
              rating: rc.rating,
              selected: rc.selectedLetter,
              domain: rc.domain,
              correctAnswer: rc.correctAnswer,
              questionText: rc.questionText
            }))
            phase = 'question'
            controlMode = pending.controlMode as Exclude<StudyMode, 'custom'>
            controlDomain = pending.controlDomain
            dueCount = data.dueCount
            newCount = data.newCount
            return
          }
        }
      } catch (_) {
        // fall through to fresh session
      }
    }

    return loadSession()
  })

  function setMode(m: Exclude<StudyMode, 'custom'>): void {
    if (controlMode === m) return
    controlMode = m
    if (m === 'group') {
      clearPendingSession(cert).catch(() => {})
      confirmingEnd = false
      saveError = null
      phase = 'empty'
      return
    }
    loadSession()
  }

  function handleModeKey(e: KeyboardEvent, m: Exclude<StudyMode, 'custom'>): void {
    if (e.key !== 'Enter' && e.key !== ' ') return
    e.preventDefault()
    setMode(m)
  }

  function selectAnswer(letter: string): void {
    if (phase !== 'question') return
    selected = letter
    phase = 'revealed'
  }

  async function rate(rating: number): Promise<void> {
    /* istanbul ignore next */
    if (phase !== 'revealed' || saving) return

    confirmingEnd = false
    const next = applyRating({ cards, current, selected, sessionResults }, rating)
    saving = true
    saveError = null

    try {
      await postReview({
        cardId: next.result.cardId,
        cert,
        rating,
        isCorrect: next.result.isCorrect,
        selected
      })

      sessionResults = next.sessionResults
      current = next.current
      selected = next.selected
      phase = next.phase

      if (next.shouldFinish) {
        await finishSession(next.sessionResults)
      }
    } catch (e) {
      saveError = (e as Error).message
    } finally {
      saving = false
    }
  }

  async function finishSession(results = sessionResults): Promise<void> {
    const total = results.length
    const correct = countCorrect(results)
    /* istanbul ignore else */
    if (total > 0) {
      await postSession({ cert, total, correct })
    }
    clearPendingSession(cert).catch(() => {})
  }

  function endSession(): void {
    clearPendingSession(cert).catch(() => {})
    ondone?.()
  }

  function handleKey(e: KeyboardEvent): void {
    const intent = keyIntent(phase, e.key, q?.options, selected === correct)
    if (!intent) return
    if (e.key === ' ' || e.key === 'Enter') e.preventDefault()
    if (intent.kind === 'select') selectAnswer(intent.letter)
    else rate(intent.rating)
  }

  let currentCard = $derived(cards[current] as AnyCard | undefined)
  let q = $derived(currentCard ? (isWrappedCard(currentCard) ? currentCard.question : currentCard as Question) : null)
  let cs = $derived((currentCard && isWrappedCard(currentCard) ? currentCard.cardState : null) as CardState | null)
  let correct = $derived(correctAnswerForCard(currentCard))
  let totalCards = $derived(cards.length)
  let correctCount = $derived(countCorrect(sessionResults))
  let accuracy = $derived(sessionAccuracy(sessionResults))
  let recentStreak = $derived(countRecentStreak(sessionResults, phase === 'revealed' && selected === correct))
  let streakMessage = $derived(messageForStreak(recentStreak))
  let domainBreakdown = $derived(phase === 'summary' ? summarizeDomains(sessionResults) : [])
  let missedCards = $derived(phase === 'summary' ? selectMissedCards(sessionResults) : [])

  const LETTERS = ['A', 'B', 'C', 'D']
</script>

<svelte:window onkeydown={handleKey} />

{#if interactive && (controlMode === 'group' || (phase !== 'loading' && phase !== 'error'))}
  <div class="study-controls">
    <div class="seg-toggle" role="group" aria-label="Study mode">
      <div
        class="seg-control {controlMode === 'due' ? 'seg-active' : ''}"
        role="button"
        tabindex="0"
        aria-pressed={controlMode === 'due'}
        onclick={() => setMode('due')}
        onkeydown={(e) => handleModeKey(e, 'due')}
      >Due</div>
      <div
        class="seg-control {controlMode === 'all' ? 'seg-active' : ''}"
        role="button"
        tabindex="0"
        aria-pressed={controlMode === 'all'}
        onclick={() => setMode('all')}
        onkeydown={(e) => handleModeKey(e, 'all')}
      >All</div>
      <div
        class="seg-control {controlMode === 'group' ? 'seg-active' : ''}"
        role="button"
        tabindex="0"
        aria-pressed={controlMode === 'group'}
        onclick={() => setMode('group')}
        onkeydown={(e) => handleModeKey(e, 'group')}
      >Group</div>
    </div>
    {#if controlMode === 'due'}
      <select class="filter-select" aria-label="Domain" bind:value={controlDomain} onchange={loadSession}>
        <option value={null}>All domains</option>
        {#each domains as d}
          <option value={d.id}>{d.name}</option>
        {/each}
      </select>
    {/if}
  </div>
{/if}

{#if interactive && controlMode === 'group'}
  <GroupHost {cert} {ondone} />

{:else if phase === 'loading'}
  <div class="loading">loading session…</div>

{:else if phase === 'error'}
  <div class="empty">Error: {error}</div>

{:else if phase === 'empty'}
  <div class="summary-box">
    <div class="summary-score">✓</div>
    <div class="summary-label">Nothing due today.</div>
    <button class="btn" onclick={() => { clearPendingSession(cert).catch(() => {}); ondone?.() }}>Back to Dashboard</button>
  </div>

{:else if phase === 'question' || phase === 'revealed'}
  {@const isCorrect = selected === correct}

  <div class="session-progress">
    <span>Card <span>{current + 1}</span> / {totalCards}</span>
    <span style="color:var(--dim)">
      {dueCount > 0 ? `${dueCount} due` : ''}
      {dueCount > 0 && newCount > 0 ? ' + ' : ''}
      {newCount > 0 ? `${newCount} new` : ''}
    </span>
  </div>

  <div class="session-bar-track">
    {#each cards as _, i}
      <div class="session-bar-segment {sessionResults[i] ? (sessionResults[i].isCorrect ? 'seg-correct' : 'seg-wrong') : ''}"></div>
    {/each}
  </div>

  {#key current}
    <div class="panel" in:fly={{ x: 16, duration: 200, easing: cubicOut }}>
      <div class="q-header">
        <span>D{q.domain} — {q.scenario}</span>
        <span class="badge {cs?.reps >= 3 ? 'badge-ok' : (cs?.due ? 'badge-due' : 'badge-new')}">
          {studyCardLabel(cs)}
        </span>
      </div>

      <div class="q-text">{q.question}</div>

      <ul class="option-list">
        {#each LETTERS as letter}
          {#if q.options && q.options[letter]}
            <li>
              <button
                class="option-item
                  {phase === 'revealed' && letter === correct ? 'reveal-correct' : ''}
                  {phase === 'revealed' && letter !== correct ? 'reveal-wrong' : ''}
                  {phase === 'revealed' && letter === selected && letter === correct ? 'selected-correct' : ''}
                  {phase === 'revealed' && letter === selected && letter !== correct ? 'selected-wrong' : ''}
                "
                disabled={phase === 'revealed'}
                onclick={() => selectAnswer(letter)}
              >
                <span class="option-key">{letter}</span>
                <span>{q.options[letter]}</span>
              </button>
            </li>
          {/if}
        {/each}
      </ul>

      {#if phase === 'revealed'}
        <div class="result-banner {isCorrect ? 'result-correct' : 'result-wrong'}" transition:fly={{ y: -6, duration: 150 }}>
          {isCorrect ? '✓ Correct' : `✗ Incorrect — correct answer: ${correct}`}
        </div>

        {#if streakMessage}
          <div class="streak-msg" transition:fade={{ duration: 200 }}>{streakMessage}</div>
        {/if}

        <div class="explanation" transition:fade={{ duration: 120, delay: 80 }}>
          {q.explanation}
          {#if q.source?.url}
            <div class="source-cite">
              <a href={q.source.url} target="_blank" rel="noopener noreferrer" class="source-link">Source ↗</a>
              {#if q.source.quote}
                <span class="source-quote">"{q.source.quote}"</span>
              {/if}
            </div>
          {/if}
        </div>

        {#if saveError}
          <div class="empty">Save failed: {saveError}</div>
        {/if}

        {#if q.tags && q.tags.length}
          <div class="tags" transition:fade={{ duration: 120, delay: 80 }}>
            {#each q.tags as tag}
              <span class="tag">#{tag}</span>
            {/each}
          </div>
        {/if}

        <div class="rating-row" transition:fade={{ duration: 120, delay: 80 }}>
          <button class="btn btn-again" disabled={saving} onclick={() => rate(1)}>Again</button>
          {#if isCorrect}
            <button class="btn btn-good" disabled={saving} onclick={() => rate(3)}>Good</button>
            <button class="btn btn-easy" disabled={saving} onclick={() => rate(4)}>Easy</button>
          {/if}
        </div>
      {/if}
    </div>
  {/key}

  <div class="end-session-row">
    {#if confirmingEnd}
      <span class="end-session-prompt">End session and lose progress?</span>
      <button class="btn btn-danger" onclick={endSession}>Yes, end it</button>
      <button class="btn" onclick={() => (confirmingEnd = false)}>Cancel</button>
    {:else}
      <button class="btn btn-end" onclick={() => (confirmingEnd = true)}>End Session</button>
    {/if}
  </div>

{:else if phase === 'summary'}
  <div class="summary-box">
    <div class="summary-score">{accuracy}%</div>
    <div class="summary-label">{correctCount} / {sessionResults.length} correct</div>
  </div>

  {#if saveError}
    <div class="empty">Save failed: {saveError}</div>
  {/if}

  {#if domainBreakdown.length > 0}
    <div class="panel" style="margin-top:16px;">
      <div class="panel-title">By Domain</div>
      {#each domainBreakdown as d}
        <div class="bar-row">
          <span class="bar-label">D{d.domain}</span>
          <div class="bar-track">
            <div class="bar-fill {barClass(d.pct)}" style="width:{d.pct}%"></div>
          </div>
          <span class="bar-pct">{d.correct}/{d.total}</span>
        </div>
      {/each}
    </div>
  {/if}

  {#if missedCards.length > 0}
    <div class="panel" style="margin-top:12px;">
      <div class="panel-title">Missed</div>
      <table class="table">
        <tbody>
          {#each missedCards as r}
            <tr>
              <td class="missed-text">{(r.questionText ?? '').slice(0, 80)}{(r.questionText ?? '').length > 80 ? '…' : ''}</td>
              <td class="missed-answer">you: {r.selected} · correct: {r.correctAnswer}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <div style="display:flex; gap:10px; justify-content:center; flex-wrap:wrap; margin-top:16px;">
    <button class="btn btn-primary" onclick={() => { clearPendingSession(cert).catch(() => {}); ondone?.() }}>Back to Dashboard</button>
    {#if controlMode === 'due' && interactive}
      <button class="btn" onclick={loadSession}>Study Again</button>
    {/if}
  </div>
{/if}
