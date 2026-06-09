// Theme tokens, presets, and persistence.
//
// The UI is fully driven by 10 CSS custom properties on `:root`. This module
// owns the values for those tokens: named presets, a hue-rotation derivation
// for free-form accent colors, and localStorage persistence. It is pure logic
// gated at 100% coverage; the single DOM touch (`applyTheme`) is exercised
// under jsdom so it stays inside the gate rather than leaking into the view.

export const TOKENS = [
  'bg', 'panel', 'panel2', 'border', 'fg', 'dim', 'muted', 'bright', 'mid', 'low'
] as const

export type TokenName = (typeof TOKENS)[number]
export type ThemeTokens = Record<TokenName, string>

// Per-token hue offset (relative to the accent hue) and lightness, measured
// from the original amber palette. Saturation is uniform across the ramp and
// supplied by the slider, so a single hue recolors the whole UI coherently.
const TOKEN_BASE: Record<TokenName, { hueOffset: number; l: number }> = {
  bg:     { hueOffset: 1,  l: 3 },
  panel:  { hueOffset: 0,  l: 5 },
  panel2: { hueOffset: 2,  l: 7 },
  border: { hueOffset: 3,  l: 12 },
  fg:     { hueOffset: 0,  l: 50 },
  dim:    { hueOffset: 3,  l: 24 },
  muted:  { hueOffset: 3,  l: 12 },
  bright: { hueOffset: 4,  l: 65 },
  mid:    { hueOffset: -8, l: 50 },
  low:    { hueOffset: -1, l: 15 }
}

const wrapHue = (h: number): number => ((h % 360) + 360) % 360

/** Rotate the dark palette to a new accent hue/saturation, keeping the
 *  lightness ramp that gives the UI its depth. */
export function deriveTheme(hue: number, sat: number): ThemeTokens {
  const out = {} as ThemeTokens
  for (const t of TOKENS) {
    const { hueOffset, l } = TOKEN_BASE[t]
    out[t] = `hsl(${wrapHue(hue + hueOffset)}, ${sat}%, ${l}%)`
  }
  return out
}

// A light/paper theme inverts the lightness ramp, so it can't be a hue rotation
// of the dark structure — it's a hand-tuned token set.
const LIGHT_THEME: ThemeTokens = {
  bg:     'hsl(42, 36%, 96%)',
  panel:  'hsl(42, 32%, 92%)',
  panel2: 'hsl(42, 30%, 87%)',
  border: 'hsl(42, 24%, 74%)',
  fg:     'hsl(40, 85%, 20%)',
  dim:    'hsl(42, 35%, 42%)',
  muted:  'hsl(42, 22%, 68%)',
  bright: 'hsl(38, 90%, 30%)',
  mid:    'hsl(32, 90%, 38%)',
  low:    'hsl(42, 40%, 80%)'
}

export type PresetName = 'amber' | 'green' | 'cyan' | 'light'

export const PRESET_NAMES: PresetName[] = ['amber', 'green', 'cyan', 'light']

export const PRESETS: Record<PresetName, ThemeTokens> = {
  amber: deriveTheme(42, 100),
  green: deriveTheme(130, 85),
  cyan:  deriveTheme(190, 80),
  light: LIGHT_THEME
}

export type ThemeState =
  | { kind: 'preset'; name: PresetName }
  | { kind: 'custom'; hue: number; sat: number }

export const DEFAULT_STATE: ThemeState = { kind: 'preset', name: 'light' }

export function resolveTokens(state: ThemeState): ThemeTokens {
  if (state.kind === 'custom') return deriveTheme(state.hue, state.sat)
  return PRESETS[state.name] ?? PRESETS.amber
}

const STORAGE_KEY = 'study-engine-theme'

function isThemeState(x: unknown): x is ThemeState {
  if (typeof x !== 'object' || x === null) return false
  const s = x as Record<string, unknown>
  if (s.kind === 'preset') return PRESET_NAMES.includes(s.name as PresetName)
  if (s.kind === 'custom') return typeof s.hue === 'number' && typeof s.sat === 'number'
  return false
}

export function loadTheme(): ThemeState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null) return DEFAULT_STATE
    const parsed = JSON.parse(raw)
    return isThemeState(parsed) ? parsed : DEFAULT_STATE
  } catch {
    return DEFAULT_STATE
  }
}

export function saveTheme(state: ThemeState): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
}

/** The lone DOM side-effect: push token values onto the document root. */
export function applyTheme(tokens: ThemeTokens): void {
  const root = document.documentElement
  for (const t of TOKENS) {
    root.style.setProperty(`--${t}`, tokens[t])
  }
}
