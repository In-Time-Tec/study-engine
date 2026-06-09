import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import Settings from './Settings.svelte'
import { DEFAULT_STATE, loadTheme, PRESETS } from './theme'
import type { BankInfo } from './types'

const api = vi.hoisted(() => ({
  fetchBanks: vi.fn(),
  uploadBank: vi.fn(),
  deleteBank: vi.fn()
}))
vi.mock('./api', () => api)

const banks: BankInfo[] = [
  { name: 'cca-f', questionCount: 60 },
  { name: 'aws-saa', questionCount: 40 }
]

function jsonFile(name: string, body = '{"cert":"x"}'): File {
  return new File([body], name, { type: 'application/json' })
}

beforeEach(() => {
  api.fetchBanks.mockReset().mockResolvedValue(banks)
  api.uploadBank.mockReset()
  api.deleteBank.mockReset()
  localStorage.clear()
})
afterEach(() => vi.unstubAllGlobals())

describe('Settings — banks', () => {
  test('lists existing banks with question counts', async () => {
    render(Settings)
    expect(await screen.findByText('cca-f')).toBeInTheDocument()
    expect(screen.getByText('60 questions')).toBeInTheDocument()
    expect(screen.getByText('aws-saa')).toBeInTheDocument()
  })

  test('shows an empty hint when no banks exist', async () => {
    api.fetchBanks.mockResolvedValue([])
    render(Settings)
    expect(await screen.findByText('No banks yet. Upload one below.')).toBeInTheDocument()
  })

  test('surfaces a bank-list load error', async () => {
    api.fetchBanks.mockRejectedValue(new Error('disk gone'))
    render(Settings)
    expect(await screen.findByText('Error: disk gone')).toBeInTheDocument()
  })
})

describe('Settings — upload', () => {
  test('uploads a chosen file and emits certsChanged with the new bank selected', async () => {
    api.uploadBank.mockResolvedValue({ ok: true, certs: ['cca-f', 'newbank'] })
    const changed = vi.fn()
    render(Settings, { props: { oncertsChanged: changed } })
    await screen.findByText('cca-f')

    const input = document.querySelector('input[type="file"]') as HTMLInputElement
    await fireEvent.change(input, { target: { files: [jsonFile('newbank.json')] } })

    await waitFor(() => expect(api.uploadBank).toHaveBeenCalledWith('newbank', '{"cert":"x"}', false))
    expect(await screen.findByText('Loaded "newbank".')).toBeInTheDocument()
    expect(changed).toHaveBeenCalledWith({ certs: ['cca-f', 'newbank'], select: 'newbank' })
  })

  test('derives a safe bank name from the filename', async () => {
    api.uploadBank.mockResolvedValue({ ok: true, certs: ['weird-name'] })
    render(Settings)
    await screen.findByText('cca-f')

    const input = document.querySelector('input[type="file"]') as HTMLInputElement
    await fireEvent.change(input, { target: { files: [jsonFile('weird name!.json')] } })

    await waitFor(() => expect(api.uploadBank).toHaveBeenCalledWith('weird-name-', '{"cert":"x"}', false))
  })

  test('accepts a file via drag-and-drop', async () => {
    api.uploadBank.mockResolvedValue({ ok: true, certs: ['dropped'] })
    render(Settings)
    await screen.findByText('cca-f')

    const zone = screen.getByLabelText('Upload a question bank JSON file')
    await fireEvent.drop(zone, { dataTransfer: { files: [jsonFile('dropped.json')] } })

    await waitFor(() => expect(api.uploadBank).toHaveBeenCalledWith('dropped', '{"cert":"x"}', false))
  })

  test('shows the server validation error on a bad bank', async () => {
    api.uploadBank.mockRejectedValue(new Error('Question q1 answer Z is not in options'))
    render(Settings)
    await screen.findByText('cca-f')

    const input = document.querySelector('input[type="file"]') as HTMLInputElement
    await fireEvent.change(input, { target: { files: [jsonFile('bad.json')] } })

    expect(await screen.findByText(/is not in options/)).toBeInTheDocument()
  })

  test('confirms before overwriting a colliding bank', async () => {
    api.uploadBank
      .mockResolvedValueOnce({ ok: false, conflict: true })
      .mockResolvedValueOnce({ ok: true, certs: ['cca-f', 'aws-saa'] })
    render(Settings)
    await screen.findByText('cca-f')

    const input = document.querySelector('input[type="file"]') as HTMLInputElement
    await fireEvent.change(input, { target: { files: [jsonFile('cca-f.json')] } })

    // First call surfaces the conflict and the confirm prompt.
    expect(await screen.findByText(/already exists/)).toBeInTheDocument()
    expect(api.uploadBank).toHaveBeenCalledTimes(1)

    await fireEvent.click(screen.getByRole('button', { name: 'Replace' }))
    await waitFor(() => expect(api.uploadBank).toHaveBeenLastCalledWith('cca-f', '{"cert":"x"}', true))
    expect(await screen.findByText('Loaded "cca-f".')).toBeInTheDocument()
  })

  test('cancelling an overwrite leaves the bank untouched', async () => {
    api.uploadBank.mockResolvedValueOnce({ ok: false, conflict: true })
    render(Settings)
    await screen.findByText('cca-f')

    const input = document.querySelector('input[type="file"]') as HTMLInputElement
    await fireEvent.change(input, { target: { files: [jsonFile('cca-f.json')] } })
    expect(await screen.findByText(/already exists/)).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByText(/already exists/)).not.toBeInTheDocument())
    expect(api.uploadBank).toHaveBeenCalledTimes(1)
  })
})

describe('Settings — delete', () => {
  test('deletes after confirmation and emits certsChanged', async () => {
    vi.stubGlobal('confirm', vi.fn(() => true))
    api.deleteBank.mockResolvedValue(['aws-saa'])
    const changed = vi.fn()
    render(Settings, { props: { oncertsChanged: changed } })
    await screen.findByText('cca-f')

    await fireEvent.click(screen.getAllByRole('button', { name: 'Delete' })[0])
    await waitFor(() => expect(api.deleteBank).toHaveBeenCalledWith('cca-f'))
    expect(changed).toHaveBeenCalledWith({ certs: ['aws-saa'] })
  })

  test('does nothing when the confirm is dismissed', async () => {
    vi.stubGlobal('confirm', vi.fn(() => false))
    render(Settings)
    await screen.findByText('cca-f')

    await fireEvent.click(screen.getAllByRole('button', { name: 'Delete' })[0])
    expect(api.deleteBank).not.toHaveBeenCalled()
  })
})

describe('Settings — theme', () => {
  test('selecting a preset applies and persists it', async () => {
    render(Settings)
    await screen.findByText('cca-f')

    await fireEvent.click(screen.getByRole('button', { name: 'cyan' }))

    expect(document.documentElement.style.getPropertyValue('--fg')).toBe(PRESETS.cyan.fg)
    expect(loadTheme()).toEqual({ kind: 'preset', name: 'cyan' })
  })

  test('dragging the hue slider switches to a custom theme and persists it', async () => {
    render(Settings)
    await screen.findByText('cca-f')

    const hue = screen.getByLabelText('Accent hue', { exact: false }) as HTMLInputElement
    await fireEvent.input(hue, { target: { value: '200' } })

    const saved = loadTheme()
    expect(saved.kind).toBe('custom')
    expect(saved).toMatchObject({ kind: 'custom', hue: 200 })
  })

  test('defaults to the stored theme on mount', async () => {
    // No stored theme → default light; mount should not crash and presets render.
    render(Settings)
    expect(await screen.findByRole('button', { name: 'amber' })).toBeInTheDocument()
    expect(loadTheme()).toEqual(DEFAULT_STATE)
  })
})
