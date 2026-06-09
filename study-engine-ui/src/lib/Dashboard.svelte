<script lang="ts">
  import { onMount } from 'svelte'
  import { slide } from 'svelte/transition'
  import { fetchStats } from './api'
  import { loadHelpCollapsed, saveHelpCollapsed } from './dashboardHelp'
  import { barClass, tagLabel } from './presentation'
  import type { Stats } from './types'

  let { cert, onstudy }: { cert: string; onstudy?: (data: { mode: string; domain: number }) => void } = $props()

  let stats: Stats | null = $state(null)
  let error: string | null = $state(null)

  let helpCollapsed: boolean = $state(loadHelpCollapsed())

  function toggleHelp(): void {
    helpCollapsed = !helpCollapsed
    saveHelpCollapsed(helpCollapsed)
  }

  onMount(async () => {
    try {
      stats = await fetchStats(cert)
    } catch (e) {
      error = (e as Error).message
    }
  })

</script>

{#if error}
  <div class="empty">Error loading stats: {error}</div>
{:else if !stats}
  <div class="loading">loading…</div>
{:else}
  <div class="panel help-panel">
    <button class="help-toggle" onclick={toggleHelp} aria-expanded={!helpCollapsed}>
      <span class="panel-title" style="margin:0">Operation manual</span>
      <span class="help-chevron">{helpCollapsed ? 'show' : 'hide'}</span>
    </button>

    {#if !helpCollapsed}
      <div class="help-body" transition:slide={{ duration: 180 }}>
        <p>
          This is a spaced-repetition tool. Every question is a card. When you
          answer one, the engine schedules when to show it again based on how
          well you knew it: miss it and it comes back soon, nail it and it spaces
          out further each time.
        </p>

        <dl class="help-defs">
          <dt>Due Today</dt>
          <dd>Cards whose review date has arrived. These are your priority.</dd>
          <dt>New Available</dt>
          <dd>Cards you haven't studied yet.</dd>
          <dt>Mastered</dt>
          <dd>Cards you've answered correctly enough times that they won't return for a while.</dd>
          <dt>Total Cards</dt>
          <dd>Every question in this bank.</dd>
        </dl>

        <p>
          The <strong>Study</strong> tab runs everything due plus a few new
          cards on the schedule. Flip its <strong>Due / All</strong> toggle to
          <strong>All</strong> for a shuffled pass over the whole bank that
          ignores scheduling, handy before an exam. Narrow a session to one area
          with the tab's domain filter, or click any bar under
          <strong>By Domain</strong> below.
        </p>

        <p>
          The <strong>By Domain</strong> and <strong>Concept Mastery</strong>
          bars show your accuracy per exam area and per tagged concept, so weak
          spots are easy to find.
        </p>

        <p>
          <strong>Recent Sessions</strong> logs one row per completed run. A
          session is only saved when you reach the last card and land on the
          summary screen. If you leave partway through, your individual answers
          are still recorded and still affect scheduling, but the run itself
          won't appear here.
        </p>
      </div>
    {/if}
  </div>

  <div class="stat-grid">
    <div class="stat-box">
      <div class="stat-value">{stats.dueToday}</div>
      <div class="stat-label">Due Today</div>
    </div>
    <div class="stat-box">
      <div class="stat-value">{stats.newAvailable}</div>
      <div class="stat-label">New Available</div>
    </div>
    <div class="stat-box">
      <div class="stat-value">{stats.mastered}</div>
      <div class="stat-label">Mastered</div>
    </div>
    <div class="stat-box">
      <div class="stat-value">{stats.total}</div>
      <div class="stat-label">Total Cards</div>
    </div>
  </div>

  <div class="panel">
    <div class="panel-title">By Domain</div>
    {#each stats.domains as d}
      <button
        type="button"
        class="bar-row bar-row-action"
        title="Study D{d.id}: {d.name}"
        onclick={() => onstudy?.({ mode: 'due', domain: d.id })}
      >
        <div class="bar-label" title="D{d.id}: {d.name}">D{d.id} {d.name}</div>
        <div class="bar-track">
          <div class="bar-fill {barClass(d.accuracy)}" style="width:{d.accuracy}%"></div>
        </div>
        <div class="bar-pct">{d.accuracy}%</div>
        <div style="font-size:10px; color:var(--dim); width:80px; text-align:right; flex-shrink:0">
          {d.mastered}/{d.total} mastered
        </div>
      </button>
    {/each}
    {#if stats.domains.every(d => d.reviewTotal === 0)}
      <div class="empty" style="padding:8px 0">No reviews yet. Start a session to see progress.</div>
    {/if}
  </div>

  {#if stats.tags.length > 0}
    <div class="panel">
      <div class="panel-title">Concept Mastery</div>
      {#each stats.tags as t}
        <div class="bar-row">
          <div class="bar-label">#{t.tag}</div>
          <div class="bar-track">
            <div class="bar-fill {barClass(t.accuracy)}" style="width:{t.accuracy}%"></div>
          </div>
          <div class="bar-pct">{t.accuracy}%</div>
          <div style="font-size:10px; color:var(--dim); width:90px; text-align:right; flex-shrink:0">
            {tagLabel(t.accuracy)}
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if stats.sessions.length > 0}
    <div class="panel">
      <div class="panel-title">Recent Sessions</div>
      <table class="table">
        <thead>
          <tr>
            <th>Date</th>
            <th>Score</th>
            <th>Accuracy</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each stats.sessions as s}
            <tr>
              <td>{s.date}</td>
              <td>{s.correct}/{s.total}</td>
              <td>{s.accuracy}%</td>
              <td>
                <div class="bar-track" style="width:100px">
                  <div class="bar-fill {barClass(s.accuracy)}" style="width:{s.accuracy}%"></div>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}
