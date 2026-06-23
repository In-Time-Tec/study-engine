<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { fetchGroupRoom, voteGroupRoom } from './api'
  import type { GroupRoomState } from './types'

  const PARTICIPANT_KEY = 'study-engine-group-participant-id'

  let {
    code,
    onleave
  }: {
    code: string
    onleave?: () => void
  } = $props()

  let roomCode: string = $state(untrack(() => code.trim().toUpperCase()))
  let participantId: string = ''
  let state: GroupRoomState | null = $state(null)
  let loading: boolean = $state(true)
  let voting: boolean = $state(false)
  let error: string | null = $state(null)
  let pollTimer: number | null = null

  let q = $derived(state?.currentQuestion ?? null)
  let isVoting = $derived(state?.status === 'voting')
  let isRevealed = $derived(state?.status === 'revealed')
  let isEnded = $derived(state?.status === 'ended')
  let selected = $derived(state?.selectedAnswer ?? null)
  let matched = $derived(Boolean(isRevealed && selected && state?.correctAnswer === selected))

  function loadParticipantId(): string {
    const existing = localStorage.getItem(PARTICIPANT_KEY)
    if (existing) return existing
    const generated = crypto.randomUUID?.() ?? `p-${Date.now()}-${Math.random().toString(36).slice(2)}`
    localStorage.setItem(PARTICIPANT_KEY, generated)
    return generated
  }

  function stopPolling(): void {
    if (pollTimer !== null) {
      window.clearInterval(pollTimer)
      pollTimer = null
    }
  }

  function ensurePolling(): void {
    if (pollTimer === null) {
      pollTimer = window.setInterval(() => {
        refreshRoom().catch(() => {})
      }, 1000)
    }
  }

  function applyRoomState(next: GroupRoomState): void {
    state = next
    if (next.status === 'ended') stopPolling()
    else ensurePolling()
  }

  async function refreshRoom(): Promise<void> {
    if (!roomCode || !participantId) return
    applyRoomState(await fetchGroupRoom({ code: roomCode, participantId }))
  }

  async function submitVote(answer: string): Promise<void> {
    if (!roomCode || !participantId || !isVoting || voting) return
    voting = true
    error = null
    try {
      applyRoomState(await voteGroupRoom({ code: roomCode, participantId, answer }))
    } catch (e) {
      error = (e as Error).message
    } finally {
      voting = false
    }
  }

  onMount(async () => {
    participantId = loadParticipantId()
    try {
      await refreshRoom()
    } catch (e) {
      error = (e as Error).message
    } finally {
      loading = false
    }
    return () => stopPolling()
  })
</script>

<div class="student-room">
  <div class="panel group-room-header student-room-header">
    <div>
      <div class="panel-title">Room</div>
      <div class="room-code">{roomCode}</div>
    </div>
    <button class="btn" onclick={() => onleave?.()}>Leave</button>
  </div>

  {#if loading}
    <div class="loading">loading room…</div>
  {:else if error}
    <div class="empty">Room unavailable: {error}</div>
  {:else if isEnded}
    <div class="summary-box">
      <div class="summary-score">Done</div>
      <div class="summary-label">Room ended</div>
    </div>
  {:else if q}
    <div class="session-progress">
      <span>Card <span>{(state?.currentIndex ?? 0) + 1}</span> / {state?.totalQuestions ?? 0}</span>
      {#if selected}
        <span style="color:var(--dim)">you chose {selected}</span>
      {/if}
    </div>

    <div class="panel">
      <div class="q-header">
        <span>D{q.domain} — {q.scenario}</span>
        <span class="badge {isRevealed ? 'badge-ok' : 'badge-new'}">{isRevealed ? 'revealed' : 'voting'}</span>
      </div>
      <div class="q-text">{q.question}</div>

      <ul class="option-list">
        {#each Object.keys(q.options).sort() as answer}
          <li>
            <button
              class="option-item
                {selected === answer ? 'selected-vote' : ''}
                {isRevealed && answer === state?.correctAnswer ? 'reveal-correct' : ''}
                {isRevealed && selected === answer && answer !== state?.correctAnswer ? 'selected-wrong' : ''}
              "
              disabled={!isVoting || voting}
              onclick={() => submitVote(answer)}
            >
              <span class="option-key">{answer}</span>
              <span>{q.options[answer]}</span>
            </button>
          </li>
        {/each}
      </ul>

      {#if isRevealed}
        <div class="result-banner {matched ? 'result-correct' : 'result-wrong'}">
          {matched ? 'Correct' : `Correct answer: ${state?.correctAnswer ?? ''}`}
        </div>
      {:else if selected}
        <div class="streak-msg">Vote recorded: {selected}</div>
      {/if}
    </div>
  {/if}
</div>
