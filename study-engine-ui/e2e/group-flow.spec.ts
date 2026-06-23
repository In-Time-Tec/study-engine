import { test, expect } from '@playwright/test'

const API = 'http://localhost:3101'
const CERT = 'e2e-demo'

test('group room collects anonymous votes without recording study progress', async ({ page, browser, request }) => {
  const before = await (await request.get(`${API}/api/stats?cert=${CERT}`)).json()

  await page.goto('/')
  await expect(page.getByText('loading…')).toHaveCount(0)
  await page.getByRole('button', { name: 'Study', exact: true }).click()
  await page.getByRole('button', { name: 'Group' }).click()
  await page.getByRole('button', { name: 'Start Room' }).click()

  const roomCode = (await page.locator('.room-code').textContent())?.trim()
  expect(roomCode).toMatch(/^[A-Z0-9]{6}$/)
  await expect(page.getByText('0 votes')).toBeVisible()

  const studentOneContext = await browser.newContext()
  const studentTwoContext = await browser.newContext()
  const studentOne = await studentOneContext.newPage()
  const studentTwo = await studentTwoContext.newPage()

  try {
    await studentOne.goto(`/?room=${roomCode}`)
    await studentTwo.goto(`/?room=${roomCode}`)
    await expect(studentOne.locator('.option-list button').first()).toBeVisible()
    await expect(studentTwo.locator('.option-list button').nth(1)).toBeVisible()

    await studentOne.locator('.option-list button').first().click()
    await studentTwo.locator('.option-list button').nth(1).click()
    await expect(page.getByText('2 votes')).toBeVisible()

    await page.getByRole('button', { name: 'Reveal' }).click()
    await expect(page.locator('.explanation')).toBeVisible()
    await expect(studentOne.locator('.result-banner')).toBeVisible()
    await expect(studentTwo.locator('.result-banner')).toBeVisible()

    const after = await (await request.get(`${API}/api/stats?cert=${CERT}`)).json()
    expect(after.introduced).toBe(before.introduced)
  } finally {
    await studentOneContext.close()
    await studentTwoContext.close()
  }
})
