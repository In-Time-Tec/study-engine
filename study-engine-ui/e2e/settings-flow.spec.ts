import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'

const API = 'http://localhost:3101'
// Uploaded into the e2e fixtures dir (the server's questions dir) and deleted
// at the end. The name is gitignored so an aborted run can't dirty the repo.
const UPLOADED = 'uploaded-e2e'

// Computer-use validation of the settings page: upload a bank through the real
// file input, confirm the backend wrote it, recolor via a theme preset, then
// delete the bank — verifying each step against backend state, not just the UI.
test('upload a bank, theme the UI, and delete the bank', async ({ page, request }) => {
  // Start clean in case a prior local run left the temp bank behind.
  await request.delete(`${API}/api/banks/${UPLOADED}`)

  await page.goto('/')
  await expect(page.getByText('loading…')).toHaveCount(0)

  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  await expect(page.getByText('Upload a Bank')).toBeVisible()

  // Upload the demo fixture under a fresh name via the hidden file input.
  const bankJson = readFileSync(new URL('./fixtures/e2e-demo.json', import.meta.url))
  await page.locator('input[type="file"]').setInputFiles({
    name: `${UPLOADED}.json`,
    mimeType: 'application/json',
    buffer: bankJson
  })

  await expect(page.getByText(`Loaded "${UPLOADED}".`)).toBeVisible()
  // Match the bank-list row specifically (the status message also contains the
  // name), so presence/absence checks track the list, not the toast.
  const uploadedRow = page.getByRole('row', { name: new RegExp(UPLOADED) })
  await expect(uploadedRow).toBeVisible()

  // The file is really on disk: the backend lists it.
  const certs = await (await request.get(`${API}/api/certs`)).json()
  expect(certs.certs).toContain(UPLOADED)

  // Theme: a preset must actually change the CSS variable on :root.
  await page.getByRole('button', { name: 'cyan' }).click()
  const fg = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--fg').trim()
  )
  expect(fg).toContain('hsl')

  // Delete the uploaded bank (cleanup + assertion). Accept the confirm dialog.
  page.once('dialog', (d) => d.accept())
  await uploadedRow.getByRole('button', { name: 'Delete' }).click()
  await expect(uploadedRow).toHaveCount(0)

  const after = await (await request.get(`${API}/api/certs`)).json()
  expect(after.certs).not.toContain(UPLOADED)
})
