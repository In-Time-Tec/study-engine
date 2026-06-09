import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import {
  TOKENS,
  PRESETS,
  PRESET_NAMES,
  DEFAULT_STATE,
  deriveTheme,
  resolveTokens,
  loadTheme,
  saveTheme,
  applyTheme,
  type ThemeState
} from './theme'

describe('deriveTheme', () => {
  test('produces an hsl value for every token', () => {
    const tokens = deriveTheme(42, 100)
    for (const t of TOKENS) {
      expect(tokens[t]).toMatch(/^hsl\(\d+(\.\d+)?, \d+%, \d+%\)$/)
    }
  })

  test('amber preset matches the original palette hues/lightness', () => {
    // fg has offset 0 → hue 42, l 50; mid has offset -8 → hue 34.
    expect(PRESETS.amber.fg).toBe('hsl(42, 100%, 50%)')
    expect(PRESETS.amber.mid).toBe('hsl(34, 100%, 50%)')
  })

  test('wraps hue into 0–359 (negative offset on low hue)', () => {
    // hue 0 with mid's -8 offset wraps to 352.
    expect(deriveTheme(0, 90).mid).toBe('hsl(352, 90%, 50%)')
  })

  test('rotating the hue changes the accent but keeps the lightness ramp', () => {
    const green = deriveTheme(130, 85)
    expect(green.fg).toBe('hsl(130, 85%, 50%)')
    expect(green.bg).toBe('hsl(131, 85%, 3%)')
  })
})

describe('PRESETS', () => {
  test('defines a full token set for every named preset', () => {
    for (const name of PRESET_NAMES) {
      for (const t of TOKENS) {
        expect(PRESETS[name][t]).toBeTruthy()
      }
    }
  })

  test('light theme is a distinct hand-tuned set, not a hue rotation', () => {
    // A light bg is impossible from the dark ramp; assert it really is light.
    expect(PRESETS.light.bg).toBe('hsl(42, 36%, 96%)')
  })
})

describe('resolveTokens', () => {
  test('resolves a preset state to its token set', () => {
    expect(resolveTokens({ kind: 'preset', name: 'cyan' })).toEqual(PRESETS.cyan)
  })

  test('resolves a custom state by deriving from hue/sat', () => {
    expect(resolveTokens({ kind: 'custom', hue: 200, sat: 70 })).toEqual(deriveTheme(200, 70))
  })

  test('falls back to amber for an unknown preset name', () => {
    const bogus = { kind: 'preset', name: 'bogus' } as unknown as ThemeState
    expect(resolveTokens(bogus)).toEqual(PRESETS.amber)
  })
})

describe('loadTheme / saveTheme', () => {
  beforeEach(() => localStorage.clear())
  afterEach(() => {
    localStorage.clear()
    vi.unstubAllGlobals()
  })

  test('returns the default when nothing is stored', () => {
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('round-trips a preset state', () => {
    const state: ThemeState = { kind: 'preset', name: 'green' }
    saveTheme(state)
    expect(loadTheme()).toEqual(state)
  })

  test('round-trips a custom state', () => {
    const state: ThemeState = { kind: 'custom', hue: 300, sat: 60 }
    saveTheme(state)
    expect(loadTheme()).toEqual(state)
  })

  test('falls back to default on malformed JSON', () => {
    localStorage.setItem('study-engine-theme', '{not json')
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('rejects a non-object payload', () => {
    localStorage.setItem('study-engine-theme', '123')
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('rejects a null payload', () => {
    localStorage.setItem('study-engine-theme', 'null')
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('rejects a preset with an unknown name', () => {
    localStorage.setItem('study-engine-theme', JSON.stringify({ kind: 'preset', name: 'nope' }))
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('rejects a custom state with non-numeric fields', () => {
    localStorage.setItem('study-engine-theme', JSON.stringify({ kind: 'custom', hue: 'x', sat: 1 }))
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })

  test('rejects an unrecognized kind', () => {
    localStorage.setItem('study-engine-theme', JSON.stringify({ kind: 'weird' }))
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })
})

describe('applyTheme', () => {
  test('writes every token onto the document root', () => {
    applyTheme(PRESETS.cyan)
    const root = document.documentElement
    expect(root.style.getPropertyValue('--fg')).toBe(PRESETS.cyan.fg)
    expect(root.style.getPropertyValue('--bg')).toBe(PRESETS.cyan.bg)
  })
})
