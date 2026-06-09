<script lang="ts">
  import { onMount } from 'svelte'
  import { fetchSessions } from './api'
  import { barClass } from './presentation'
  import type { SessionRecord } from './types'

  let { cert }: { cert: string } = $props()

  let sessions: SessionRecord[] = $state([])
  let error: string | null = $state(null)
  let loading = $state(true)

  onMount(async () => {
    try {
      const data = await fetchSessions({ cert, limit: 50 })
      sessions = data.sessions || []
    } catch (e) {
      error = (e as Error).message
    } finally {
      loading = false
    }
  })

</script>

{#if loading}
  <div class="empty">Loading…</div>
{:else if error}
  <div class="empty">Error: {error}</div>
{:else if sessions.length === 0}
  <div class="empty">No sessions recorded yet.</div>
{:else}
  <div class="panel" style="padding:0; overflow:hidden;">
    <table class="table">
      <thead>
        <tr>
          <th>Date</th>
          <th>Score</th>
          <th>Accuracy</th>
          <th style="width:120px"></th>
        </tr>
      </thead>
      <tbody>
        {#each sessions as s}
          <tr>
            <td>{s.date}</td>
            <td>{s.correct} / {s.total}</td>
            <td style="color:{s.accuracy >= 80 ? 'var(--bright)' : s.accuracy >= 50 ? 'var(--mid)' : 'var(--dim)'}">
              {s.accuracy}%
            </td>
            <td>
              <div class="bar-track" style="width:120px">
                <div class="bar-fill {barClass(s.accuracy)}" style="width:{s.accuracy}%"></div>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
