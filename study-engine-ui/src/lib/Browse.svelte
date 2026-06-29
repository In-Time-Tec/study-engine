<script lang="ts">
  import { onMount } from 'svelte'
  import { fetchQuestions } from './api'
  import { allTags as selectAllTags, filterQuestions } from './browseSelectors'
  import { browseCardBadge } from './presentation'
  import type { QuestionsResponse } from './types'

  let { cert, onquiz }: { cert: string; onquiz?: (data: { questionIds: string[] }) => void } = $props()

  let allData: QuestionsResponse | null = $state(null)
  let error: string | null = $state(null)
  let expanded: Record<string, boolean> = $state({})

  let filterDomain: string = $state('')
  let filterTag: string = $state('')
  let filterSearch: string = $state('')

  onMount(async () => {
    try {
      allData = await fetchQuestions({ cert })
    } catch (e) {
      error = (e as Error).message
    }
  })

  let filtered = $derived(allData
    ? filterQuestions(allData.questions, {
        domain: filterDomain,
        tag: filterTag,
        search: filterSearch
      })
    : [])

  function toggle(id: string): void {
    expanded[id] = !expanded[id]
  }

  function quizFiltered(): void {
    const ids = filtered.map(({ question }) => question.id)
    onquiz?.({ questionIds: ids })
  }

  let allTags = $derived(allData
    ? selectAllTags(allData.questions)
    : [])
</script>

{#if error}
  <div class="empty">Error: {error}</div>
{:else if !allData}
  <div class="loading">loading…</div>
{:else}
  <div class="filter-bar">
    <select class="filter-select" bind:value={filterDomain}>
      <option value="">All Domains</option>
      {#each Object.entries(allData.domains) as [id, name]}
        <option value={id}>D{id} — {name}</option>
      {/each}
    </select>

    <select class="filter-select" bind:value={filterTag}>
      <option value="">All Tags</option>
      {#each allTags as tag}
        <option value={tag}>#{tag}</option>
      {/each}
    </select>

    <input
      class="filter-input"
      type="text"
      placeholder="Search question or scenario…"
      bind:value={filterSearch}
    />

    <button class="btn btn-primary" disabled={filtered.length === 0} onclick={quizFiltered}>
      Quiz {filtered.length} cards
    </button>
  </div>

  <div style="font-size:11px; color:var(--dim); margin-bottom:12px;">
    {filtered.length} of {allData.questions.length} questions
  </div>

  <div class="panel" style="padding:0; overflow:hidden;">
    {#if filtered.length === 0}
      <div class="empty">No questions match this filter.</div>
    {:else}
      {#each filtered as { question: q, cardState: cs }}
        {@const badge = browseCardBadge(cs)}
        <div class="q-row">
          <div class="q-row-header" onclick={() => toggle(q.id)} role="button" tabindex="0"
            onkeydown={e => e.key === 'Enter' && toggle(q.id)}>
            <div class="q-row-domain">D{q.domain}</div>
            <div class="q-row-text">
              <div style="font-size:10px; color:var(--mid); margin-bottom:3px;">{q.scenario}</div>
              <div>{q.question.length > 120 ? q.question.slice(0, 120) + '…' : q.question}</div>
            </div>
            <div class="q-row-meta">
              <span class="badge {badge.cls}">{badge.text}</span>
              <span class="q-row-expand">{expanded[q.id] ? '▲' : '▼'}</span>
            </div>
          </div>

          {#if expanded[q.id]}
            <div class="q-detail">
              <div style="margin-bottom:10px; color:var(--fg);">{q.question}</div>
              <ul class="option-list">
                {#each ['A','B','C','D'] as letter}
                  {#if q.options && q.options[letter]}
                    <li>
                      <div class="option-item {letter === q.answer ? 'reveal-correct' : 'reveal-wrong'}" style="cursor:default">
                        <span class="option-key">{letter}</span>
                        <span>{q.options[letter]}</span>
                      </div>
                    </li>
                  {/if}
                {/each}
              </ul>
              <div class="q-answer">
                <strong style="color:var(--bright)">Answer: {q.answer}</strong>
                <div style="margin-top:6px; color:var(--fg); line-height:1.7">
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
              </div>
              {#if q.tags && q.tags.length}
                <div class="tags" style="margin-top:8px;">
                  {#each q.tags as tag}
                    <span class="tag">#{tag}</span>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
{/if}
