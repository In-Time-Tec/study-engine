<script lang="ts">
  import { onMount } from 'svelte'
  import Dashboard from './lib/Dashboard.svelte'
  import StudySession from './lib/StudySession.svelte'
  import Browse from './lib/Browse.svelte'
  import History from './lib/History.svelte'
  import Settings from './lib/Settings.svelte'
  import { fetchCerts } from './lib/api'

  let tab: string = $state('dashboard')
  let studyMode: string = $state('due')
  let studyDomain: number | null = $state(null)
  let quizIds: string[] | null = $state(null)

  let certs: string[] = $state([])
  let cert: string = $state('')
  let certsLoading = $state(true)

  onMount(async () => {
    try {
      applyCerts(await fetchCerts())
    } catch {
      applyCerts([])
    } finally {
      certsLoading = false
    }
  })

  // Adopt a refreshed cert list. Prefer an explicitly requested bank (e.g. one
  // just uploaded), otherwise keep the current selection if it survived, else
  // fall back to the first available bank.
  function applyCerts(next: string[], select?: string): void {
    certs = next
    if (select && next.includes(select)) cert = select
    else if (!cert || !next.includes(cert)) cert = next[0] ?? ''
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
  }

  function startDefaultStudy(): void {
    studyMode = 'due'
    studyDomain = null
    quizIds = null
    tab = 'study'
  }
</script>

<div class="header">
  <div class="header-brand">STUDY ENGINE</div>
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
</div>

<div class="content">
  {#if certsLoading}
    <div class="loading">loading…</div>

  {:else if tab === 'settings'}
    <Settings oncertsChanged={onCertsChanged} />

  {:else if !cert}
    <div class="empty">
      <p>No question bank yet.</p>
      <p>Open <button class="link-btn" onclick={() => (tab = 'settings')}>Settings</button> to upload your first <code>.json</code> bank.</p>
    </div>

  {:else}
    {#if certs.length > 1}
      <div style="padding: 8px 16px; display:flex; align-items:center; gap:8px; font-size:13px;">
        <label for="cert-select">Bank:</label>
        <select id="cert-select" class="filter-select" bind:value={cert}>{#each certs as c}<option value={c}>{c}</option>{/each}</select>
      </div>
    {/if}

    {#if tab === 'dashboard'}
      <Dashboard {cert} onstudy={goStudy} />

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
