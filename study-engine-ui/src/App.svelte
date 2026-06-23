<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Dashboard from './lib/Dashboard.svelte'
  import StudySession from './lib/StudySession.svelte'
  import Browse from './lib/Browse.svelte'
  import GroupStudent from './lib/GroupStudent.svelte'
  import History from './lib/History.svelte'
  import Settings from './lib/Settings.svelte'
  import { fetchCerts, fetchStats } from './lib/api'
  import { nextSessionDueText } from './lib/presentation'
  import { loadSelectedCert, saveSelectedCert } from './lib/certSelection'
  import type { Stats } from './lib/types'

  let tab: string = $state('dashboard')
  let roomCode: string | null = $state(null)
  let studyMode: string = $state('due')
  let studyDomain: number | null = $state(null)
  let quizIds: string[] | null = $state(null)

  let certs: string[] = $state([])
  let cert: string = $state('')
  let certsLoading = $state(true)
  let headerStats: Stats | null = $state(null)
  let headerStatsLoading = $state(false)
  let headerStatsError: string | null = $state(null)
  let statsRequest = 0

  let nextSessionText = $derived(
    nextSessionDueText(headerStats, certsLoading || headerStatsLoading, headerStatsError, Boolean(cert))
  )

  onMount(async () => {
    roomCode = new URLSearchParams(window.location.search).get('room')
    try {
      applyCerts(await fetchCerts())
    } catch {
      applyCerts([])
    } finally {
      certsLoading = false
    }
  })

  $effect(() => {
    const selectedCert = cert
    untrack(() => {
      if (selectedCert) saveSelectedCert(selectedCert)
      void refreshHeaderStats(selectedCert)
    })
  })

  // Adopt a refreshed cert list. Prefer an explicitly requested bank (e.g. one
  // just uploaded), then the current selection if it survived, then the last
  // selection persisted across reloads, else the first available bank.
  function applyCerts(next: string[], select?: string): void {
    certs = next
    if (select && next.includes(select)) cert = select
    else if (!cert || !next.includes(cert)) {
      const stored = loadSelectedCert()
      cert = stored && next.includes(stored) ? stored : (next[0] ?? '')
    }
  }

  function onCertsChanged(data: { certs: string[]; select?: string }): void {
    applyCerts(data.certs, data.select)
  }

  interface Tab { id: string; label: string }
  const tabs: Tab[] = [
    { id: 'dashboard', label: 'Dashboard' },
    { id: 'study',     label: 'Study' },
    { id: 'browse',    label: 'Browse' },
    { id: 'history',   label: 'History' },
    { id: 'settings',  label: 'Settings' },
  ]

  function goStudy(data: { mode: string; domain: number | null }): void {
    studyMode = data?.mode || 'due'
    studyDomain = data?.domain ?? null
    quizIds = null
    tab = 'study'
  }

  function goQuiz(data: { questionIds: string[] }): void {
    studyMode = 'custom'
    quizIds = data?.questionIds || null
    tab = 'study'
  }

  function studyDone(): void {
    tab = 'dashboard'
    void refreshHeaderStats(cert)
  }

  function startDefaultStudy(): void {
    studyMode = 'due'
    studyDomain = null
    quizIds = null
    tab = 'study'
  }

  function leaveRoom(): void {
    roomCode = null
    if (typeof window !== 'undefined') {
      window.history.replaceState(null, '', window.location.pathname)
    }
  }

  async function refreshHeaderStats(selectedCert: string): Promise<void> {
    const request = ++statsRequest
    if (!selectedCert) {
      headerStats = null
      headerStatsLoading = false
      headerStatsError = null
      return
    }

    headerStatsLoading = true
    headerStatsError = null
    try {
      const stats = await fetchStats(selectedCert)
      if (request !== statsRequest) return
      headerStats = stats
    } catch (e) {
      if (request !== statsRequest) return
      headerStats = null
      headerStatsError = (e as Error).message
    } finally {
      if (request === statsRequest) headerStatsLoading = false
    }
  }
</script>

<div class="header">
  <div class="header-brand">STUDY ENGINE</div>
  {#if !roomCode}
    <nav class="nav">
      {#each tabs as t}
        <button
          class="nav-btn {tab === t.id ? 'active' : ''}"
          onclick={() => { if (t.id === 'study') { if (tab !== 'study') startDefaultStudy() } else { tab = t.id } }}
        >
          {t.label}
        </button>
      {/each}
    </nav>
    <div
      class="session-due"
      title={headerStatsError ? `Next session unavailable: ${headerStatsError}` : 'Next session due'}
    >
      <span class="session-due-label">Next session</span>
      <span class="session-due-value">{nextSessionText}</span>
    </div>
  {/if}
</div>

<div class="content">
  {#if roomCode}
    <GroupStudent code={roomCode} onleave={leaveRoom} />

  {:else if certsLoading}
    <div class="loading">loading…</div>

  {:else if tab === 'settings'}
    <Settings oncertsChanged={onCertsChanged} {cert} {certs} />

  {:else if !cert}
    <div class="empty">
      <p>No question bank yet.</p>
      <p>Open <button class="link-btn" onclick={() => (tab = 'settings')}>Settings</button> to upload your first <code>.json</code> bank.</p>
    </div>

  {:else}
    {#if tab === 'dashboard'}
      {#key cert}
        <Dashboard {cert} onstudy={goStudy} />
      {/key}

    {:else if tab === 'study'}
      {#key studyMode + '|' + (quizIds ? quizIds.join(',') : '') + '|' + (studyDomain ?? '') + '|' + cert}
        <StudySession
          {cert}
          mode={studyMode}
          domain={studyDomain}
          questionIds={quizIds}
          ondone={studyDone}
        />
      {/key}

    {:else if tab === 'browse'}
      <Browse {cert} onquiz={goQuiz} />

    {:else if tab === 'history'}
      <History {cert} />
    {/if}
  {/if}
</div>

<footer class="footer">
  study-engine — a local-first spaced-repetition tool. Bring your own question bank.
</footer>
