import { beforeEach, describe, expect, it } from 'vitest'
import { loadSelectedCert, saveSelectedCert } from './certSelection'

describe('certSelection persistence', () => {
  beforeEach(() => localStorage.clear())

  it('returns null when nothing is stored', () => {
    expect(loadSelectedCert()).toBe(null)
  })

  it('round-trips a selected cert', () => {
    saveSelectedCert('my-cert')
    expect(loadSelectedCert()).toBe('my-cert')
  })
})
