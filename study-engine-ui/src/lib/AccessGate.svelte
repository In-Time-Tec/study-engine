<script lang="ts">
  import { onMount } from 'svelte'

  let { onready }: { onready: () => void } = $props()

  let requiresCode = $state(false)
  let checking = $state(true)
  let codeInput = $state('')
  let nameInput = $state('')
  let error = $state('')
  let submitting = $state(false)

  onMount(async () => {
    const storedCode = localStorage.getItem('accessCode') ?? ''
    const storedName = localStorage.getItem('userName') ?? ''

    try {
      const r = await fetch('/api/config')
      const cfg = await r.json() as { requiresCode: boolean }
      requiresCode = cfg.requiresCode

      if (storedName && (!cfg.requiresCode || storedCode)) {
        if (cfg.requiresCode) {
          // Verify stored code is still valid
          const vr = await fetch('/api/verify-code', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code: storedCode })
          })
          if (vr.ok) {
            onready()
            return
          }
          // Stored code is stale — prompt again
          localStorage.removeItem('accessCode')
        } else {
          onready()
          return
        }
      }
    } catch {
      // If /api/config fails, assume no code required and proceed with name only
    }
    checking = false
    codeInput = ''
    nameInput = ''
  })

  async function submit() {
    error = ''
    const name = nameInput.trim()
    if (!name) { error = 'Please enter your name.'; return }
    if (requiresCode && !codeInput.trim()) { error = 'Please enter the access code.'; return }

    submitting = true
    try {
      if (requiresCode) {
        const r = await fetch('/api/verify-code', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ code: codeInput.trim() })
        })
        if (!r.ok) { error = 'Invalid access code.'; return }
        localStorage.setItem('accessCode', codeInput.trim())
      }
      localStorage.setItem('userName', name)
      onready()
    } catch {
      error = 'Network error — try again.'
    } finally {
      submitting = false
    }
  }
</script>

{#if checking}
  <div class="gate-wrap"><p class="gate-hint">Loading…</p></div>
{:else}
  <div class="gate-wrap">
    <div class="gate-card">
      <h2 class="gate-title">STUDY ENGINE</h2>
      <form onsubmit={(e) => { e.preventDefault(); void submit() }}>
        {#if requiresCode}
          <label class="gate-label">
            Access code
            <input
              class="gate-input"
              type="password"
              placeholder="Enter access code"
              bind:value={codeInput}
              autocomplete="off"
            />
          </label>
        {/if}
        <label class="gate-label">
          Your name
          <input
            class="gate-input"
            type="text"
            placeholder="e.g. Alex"
            bind:value={nameInput}
            autocomplete="given-name"
          />
        </label>
        {#if error}
          <p class="gate-error">{error}</p>
        {/if}
        <button class="gate-btn" type="submit" disabled={submitting}>
          {submitting ? 'Checking…' : 'Enter'}
        </button>
      </form>
    </div>
  </div>
{/if}

<style>
  .gate-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    padding: 2rem;
  }

  .gate-card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 2rem 2.5rem;
    width: 100%;
    max-width: 360px;
  }

  .gate-title {
    font-size: 1.25rem;
    font-weight: 700;
    color: var(--bright);
    letter-spacing: 0.1em;
    margin: 0 0 1.5rem;
    text-align: center;
  }

  .gate-label {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    font-size: 0.85rem;
    color: var(--dim);
    margin-bottom: 1rem;
  }

  .gate-input {
    background: var(--panel2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5rem 0.75rem;
    color: var(--fg);
    font-size: 0.9rem;
    width: 100%;
    box-sizing: border-box;
  }

  .gate-input:focus {
    outline: none;
    border-color: var(--mid);
  }

  .gate-error {
    color: #f87171;
    font-size: 0.8rem;
    margin: 0 0 0.75rem;
  }

  .gate-btn {
    width: 100%;
    padding: 0.6rem 1rem;
    background: var(--mid);
    color: var(--bg);
    border: none;
    border-radius: 4px;
    font-size: 0.9rem;
    font-weight: 600;
    cursor: pointer;
    margin-top: 0.25rem;
  }

  .gate-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .gate-hint {
    color: var(--dim);
    font-size: 0.9rem;
  }
</style>
