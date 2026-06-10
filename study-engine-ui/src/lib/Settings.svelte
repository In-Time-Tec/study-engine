<script lang="ts">
  import { onMount } from 'svelte'
  import { fetchBanks, uploadBank, deleteBank } from './api'
  import {
    PRESET_NAMES,
    applyTheme,
    resolveTokens,
    loadTheme,
    saveTheme
  } from './theme'
  import type { ThemeState, PresetName } from './theme'
  import type { BankInfo } from './types'

  let { oncertsChanged, cert = '', certs = [] }: {
    oncertsChanged?: (data: { certs: string[]; select?: string }) => void
    cert?: string
    certs?: string[]
  } = $props()

  // ─── Banks ──────────────────────────────────────────────────────────────
  let banks: BankInfo[] = $state([])
  let banksError: string | null = $state(null)

  let dragging = $state(false)
  let uploadError: string | null = $state(null)
  let uploadStatus: string | null = $state(null)
  // Holds the parsed file while we wait for the user to confirm a replace.
  let pending: { name: string; content: string } | null = $state(null)
  let fileInput: HTMLInputElement

  onMount(loadBanks)

  async function loadBanks(): Promise<void> {
    try {
      banks = await fetchBanks()
      banksError = null
    } catch (e) {
      banksError = (e as Error).message
    }
  }

  // Default the cert id to the file's stem, sanitized to the slug the server
  // accepts. The user never has to think about the questions/ folder.
  function bankNameFromFile(filename: string): string {
    return filename.replace(/\.json$/i, '').replace(/[^A-Za-z0-9_-]/g, '-')
  }

  async function handleFile(file: File): Promise<void> {
    uploadError = null
    uploadStatus = null
    pending = null
    const content = await file.text()
    await send(bankNameFromFile(file.name), content, false)
  }

  async function send(name: string, content: string, overwrite: boolean): Promise<void> {
    try {
      const result = await uploadBank(name, content, overwrite)
      if (!result.ok) {
        pending = { name, content }
        return
      }
      pending = null
      uploadStatus = `Loaded "${name}".`
      await loadBanks()
      oncertsChanged?.({ certs: result.certs, select: name })
    } catch (e) {
      uploadError = (e as Error).message
    }
  }

  function confirmOverwrite(): void {
    if (pending) void send(pending.name, pending.content, true)
  }
  function cancelOverwrite(): void {
    pending = null
  }

  function onDrop(e: DragEvent): void {
    e.preventDefault()
    dragging = false
    const file = e.dataTransfer?.files?.[0]
    if (file) void handleFile(file)
  }
  function onFileChange(e: Event): void {
    const file = (e.target as HTMLInputElement).files?.[0]
    if (file) void handleFile(file)
  }

  async function remove(name: string): Promise<void> {
    if (!confirm(`Delete "${name}"? Review history is kept and resumes if you re-upload it.`)) return
    try {
      const certs = await deleteBank(name)
      await loadBanks()
      oncertsChanged?.({ certs })
    } catch (e) {
      banksError = (e as Error).message
    }
  }

  // ─── Theme ──────────────────────────────────────────────────────────────
  const loadedTheme = loadTheme()
  let themeState: ThemeState = $state(loadedTheme)
  let hue = $state(loadedTheme.kind === 'custom' ? loadedTheme.hue : 42)
  let sat = $state(loadedTheme.kind === 'custom' ? loadedTheme.sat : 100)

  function applyAndSave(): void {
    applyTheme(resolveTokens(themeState))
    saveTheme(themeState)
  }
  function choosePreset(name: PresetName): void {
    themeState = { kind: 'preset', name }
    applyAndSave()
  }
  function chooseCustom(): void {
    themeState = { kind: 'custom', hue, sat }
    applyAndSave()
  }
  let activePreset = $derived(themeState.kind === 'preset' ? themeState.name : null)
</script>

<div class="settings">
  <div class="panel">
    <div class="panel-title">Question Banks</div>
    {#if certs.length > 1}
      <div style="display:flex; align-items:center; gap:8px; font-size:13px; margin-bottom:12px;">
        <label for="cert-select">Active bank</label>
        <select
          id="cert-select"
          class="filter-select"
          value={cert}
          onchange={(e) => oncertsChanged?.({ certs, select: (e.target as HTMLSelectElement).value })}
        >
          {#each certs as c}<option value={c}>{c}</option>{/each}
        </select>
      </div>
    {/if}
    {#if banksError}
      <div class="empty">Error: {banksError}</div>
    {:else if banks.length === 0}
      <div class="settings-hint">No banks yet. Upload one below.</div>
    {:else}
      <table class="bank-table">
        <tbody>
          {#each banks as b}
            <tr>
              <td class="bank-name">{b.name}</td>
              <td class="bank-count">{b.questionCount} questions</td>
              <td class="bank-actions">
                <button class="btn" onclick={() => remove(b.name)}>Delete</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>

  <div class="panel">
    <div class="panel-title">Upload a Bank</div>
    <div
      class="dropzone {dragging ? 'dragging' : ''}"
      role="button"
      tabindex="0"
      aria-label="Upload a question bank JSON file"
      onclick={() => fileInput.click()}
      onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && fileInput.click()}
      ondragover={(e) => { e.preventDefault(); dragging = true }}
      ondragleave={() => (dragging = false)}
      ondrop={onDrop}
    >
      Drop a <code>.json</code> bank here, or click to choose a file.
    </div>
    <input
      bind:this={fileInput}
      type="file"
      accept=".json,application/json"
      style="display:none"
      onchange={onFileChange}
    />

    {#if uploadStatus}<div class="upload-ok">{uploadStatus}</div>{/if}
    {#if uploadError}<div class="upload-err">{uploadError}</div>{/if}
    {#if pending}
      <div class="upload-warn">
        A bank named "{pending.name}" already exists. Replacing it may orphan saved
        progress if its questions changed.
        <div class="upload-warn-actions">
          <button class="btn btn-primary" onclick={confirmOverwrite}>Replace</button>
          <button class="btn" onclick={cancelOverwrite}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>

  <div class="panel">
    <div class="panel-title">Theme</div>
    <div class="theme-presets">
      {#each PRESET_NAMES as name}
        <button
          class="btn {activePreset === name ? 'btn-primary' : ''}"
          onclick={() => choosePreset(name)}
        >
          {name}
        </button>
      {/each}
    </div>
    <div class="theme-sliders">
      <label class="theme-slider">
        <span>Accent hue</span>
        <input type="range" min="0" max="360" bind:value={hue} oninput={chooseCustom} />
        <span class="theme-value">{hue}°</span>
      </label>
      <label class="theme-slider">
        <span>Saturation</span>
        <input type="range" min="0" max="100" bind:value={sat} oninput={chooseCustom} />
        <span class="theme-value">{sat}%</span>
      </label>
    </div>
  </div>
</div>
