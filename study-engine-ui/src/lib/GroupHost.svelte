<script lang="ts">
  import { onMount } from 'svelte'
  import { createGroupRoom, endGroupRoom, fetchGroupRoom, nextGroupRoom, prevGroupRoom, revealGroupRoom } from './api'
  import type { GroupRoomState } from './types'

  let {
    cert = 'cca-f',
    ondone
  }: {
    cert?: string
    ondone?: () => void
  } = $props()

  let state: GroupRoomState | null = $state(null)
  let roomCode: string = $state('')
  let hostToken: string = $state('')
  let joinUrl: string = $state('')
  let error: string | null = $state(null)
  let loading: boolean = $state(false)
  let busy: boolean = $state(false)
  let copyLabel: string = $state('Copy')
  let pollTimer: number | null = null

  let q = $derived(state?.currentQuestion ?? null)
  let isVoting = $derived(state?.status === 'voting')
  let isRevealed = $derived(state?.status === 'revealed')
  let isEnded = $derived(state?.status === 'ended')
  let hasNext = $derived(Boolean(state && state.currentIndex < state.totalQuestions - 1))
  let hasPrev = $derived(Boolean(state && state.currentIndex > 0))

  function browserJoinUrl(code: string, fallback: string): string {
    if (typeof window === 'undefined') return fallback
    return `${window.location.origin}/?room=${code}`
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
    if (!roomCode || !hostToken) return
    applyRoomState(await fetchGroupRoom({ code: roomCode, hostToken }))
  }

  async function startRoom(): Promise<void> {
    loading = true
    error = null
    try {
      const created = await createGroupRoom(cert)
      roomCode = created.code
      hostToken = created.hostToken
      joinUrl = browserJoinUrl(created.code, created.joinUrl)
      applyRoomState(created.state)
    } catch (e) {
      error = (e as Error).message
    } finally {
      loading = false
    }
  }

  async function reveal(): Promise<void> {
    if (!roomCode || !hostToken || busy) return
    busy = true
    error = null
    try {
      applyRoomState(await revealGroupRoom(roomCode, hostToken))
    } catch (e) {
      error = (e as Error).message
    } finally {
      busy = false
    }
  }

  async function next(): Promise<void> {
    if (!roomCode || !hostToken || busy) return
    busy = true
    error = null
    try {
      applyRoomState(await nextGroupRoom(roomCode, hostToken))
    } catch (e) {
      error = (e as Error).message
    } finally {
      busy = false
    }
  }

  async function prev(): Promise<void> {
    if (!roomCode || !hostToken || busy) return
    busy = true
    error = null
    try {
      applyRoomState(await prevGroupRoom(roomCode, hostToken))
    } catch (e) {
      error = (e as Error).message
    } finally {
      busy = false
    }
  }

  async function endRoom(): Promise<void> {
    if (!roomCode || !hostToken || busy) return
    busy = true
    error = null
    try {
      applyRoomState(await endGroupRoom(roomCode, hostToken))
    } catch (e) {
      error = (e as Error).message
    } finally {
      busy = false
    }
  }

  async function copyJoinLink(): Promise<void> {
    try {
      await navigator.clipboard.writeText(joinUrl)
      copyLabel = 'Copied'
      window.setTimeout(() => (copyLabel = 'Copy'), 1200)
    } catch {
      copyLabel = 'Copy failed'
      window.setTimeout(() => (copyLabel = 'Copy'), 1200)
    }
  }

  function pct(count: number): number {
    if (!state || state.totalVotes === 0) return 0
    return Math.round((count / state.totalVotes) * 100)
  }

  onMount(() => () => stopPolling())
</script>

{#if !state}
  <div class="panel group-start">
    <div class="panel-title">Group</div>
    <div class="group-start-row">
      <div>
        <div class="group-start-title">All cards, shuffled</div>
        <div class="group-muted">No review progress is saved.</div>
      </div>
      <button class="btn btn-primary" disabled={loading} onclick={startRoom}>
        {loading ? 'Starting…' : 'Start Room'}
      </button>
    </div>
    {#if error}
      <div class="empty">Group room failed: {error}</div>
    {/if}
  </div>
{:else}
  <div class="group-room-shell">
    <div class="panel group-room-header">
      <div>
        <div class="panel-title">Room</div>
        <div class="room-code">{roomCode}</div>
      </div>
      <div class="join-link-wrap">
        <label class="join-link-label" for="group-join-link">Join link</label>
        <div class="join-link-row">
          <input id="group-join-link" class="join-link-input" readonly value={joinUrl} />
          <button class="btn" onclick={copyJoinLink}>{copyLabel}</button>
        </div>
      </div>
      <button class="btn btn-end" disabled={busy || isEnded} onclick={endRoom}>End</button>
    </div>

    {#if error}
      <div class="empty">Group room failed: {error}</div>
    {/if}

    {#if isEnded}
      <div class="summary-box">
        <div class="summary-score">Done</div>
        <div class="summary-label">Room ended</div>
        <button class="btn btn-primary" onclick={() => ondone?.()}>Back to Dashboard</button>
      </div>
    {:else if q}
      <div class="session-progress">
        <span>Card <span>{state.currentIndex + 1}</span> / {state.totalQuestions}</span>
        <span style="color:var(--dim)">{state.totalVotes} answered</span>
      </div>

      <div class="panel">
        <div class="q-header">
          <span>D{q.domain} — {q.scenario}</span>
          <span class="badge {isRevealed ? 'badge-ok' : 'badge-new'}">{isRevealed ? 'revealed' : 'voting'}</span>
        </div>
        <div class="q-text">{q.question}</div>

        <div class="vote-list">
          {#each state.voteCounts as vote}
            <div class="vote-row {isRevealed ? '' : 'vote-row-hidden'}">
              <div class="vote-answer">
                <span class="option-key">{vote.answer}</span>
                <span>{q.options[vote.answer]}</span>
              </div>
              {#if isRevealed}
                <div class="vote-track">
                  <div class="vote-fill" style="width:{pct(vote.count)}%"></div>
                </div>
                <div class="vote-count">{vote.count}</div>
              {/if}
            </div>
          {/each}
        </div>

        {#if isRevealed}
          <div class="result-banner result-correct">Correct answer: {state.correctAnswer}</div>
          {#if state.explanation}
            <div class="explanation">{state.explanation}</div>
          {/if}
        {/if}

        <div class="rating-row">
          {#if hasPrev}
            <button class="btn" disabled={busy} onclick={prev}>← Back</button>
          {/if}
          {#if isVoting}
            <button class="btn btn-primary" disabled={busy} onclick={reveal}>Reveal</button>
          {:else if isRevealed && hasNext}
            <button class="btn btn-primary" disabled={busy} onclick={next}>Next</button>
          {:else if isRevealed}
            <button class="btn btn-primary" disabled={busy} onclick={endRoom}>End Room</button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}
