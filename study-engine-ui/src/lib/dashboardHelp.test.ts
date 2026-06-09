import { beforeEach, describe, expect, it } from 'vitest'
import { loadHelpCollapsed, saveHelpCollapsed } from './dashboardHelp'

describe('dashboardHelp persistence', () => {
  beforeEach(() => localStorage.clear())

  it('defaults to collapsed when nothing is stored', () => {
    expect(loadHelpCollapsed()).toBe(true)
  })

  it('round-trips a collapsed state', () => {
    saveHelpCollapsed(true)
    expect(loadHelpCollapsed()).toBe(true)
  })

  it('round-trips an expanded state', () => {
    saveHelpCollapsed(false)
    expect(loadHelpCollapsed()).toBe(false)
  })
})
