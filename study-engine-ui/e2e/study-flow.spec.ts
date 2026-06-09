import { test, expect } from '@playwright/test'

const API = 'http://localhost:3101'
const CERT = 'e2e-demo'

// Computer-use validation: drive the real browser through a study review, then
// confirm the review actually persisted in the backend — not just that the UI
// called the API. This is the gap the mocked component tests can't cover.
test('reviewing a card persists to the backend', async ({ page, request }) => {
  // Baseline: a fresh DB has introduced nothing yet.
  const before = await (await request.get(`${API}/api/stats?cert=${CERT}`)).json()
  expect(before.introduced).toBe(0)

  await page.goto('/')

  // Wait for the cert list to load (the bank auto-selects) before navigating.
  await expect(page.getByText('loading…')).toHaveCount(0)
  await page.getByRole('button', { name: 'Study', exact: true }).click()

  // The first new card from the fixture bank.
  await expect(page.getByText('What is 2 + 2?')).toBeVisible()

  // Answer correctly, then rate — and wait for the review POST to complete.
  await page.getByRole('button', { name: 'Four' }).click()
  const [reviewResponse] = await Promise.all([
    page.waitForResponse(
      (r) => r.url().includes('/api/review') && r.request().method() === 'POST'
    ),
    page.getByRole('button', { name: 'Good' }).click()
  ])
  expect(reviewResponse.status()).toBe(200)

  // Backend state must reflect the review: demo-1 is now introduced.
  const after = await (await request.get(`${API}/api/stats?cert=${CERT}`)).json()
  expect(after.introduced).toBe(1)
})
