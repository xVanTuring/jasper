import { test, expect } from '@playwright/test'
import { openApp } from './helpers'

test('search filters notes by title', async ({ page }) => {
	await openApp(page)
	await page.getByRole('searchbox').fill('Todo')
	await expect(page.locator('button.note', { hasText: 'Todo Note' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Plain Note' })).toHaveCount(0)
})

test('search matches body text too', async ({ page }) => {
	await openApp(page)
	// "Plain Note" 正文含 markdown 一词，"Image Note" 不含
	await page.getByRole('searchbox').fill('markdown')
	await expect(page.locator('button.note', { hasText: 'Plain Note' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Image Note' })).toHaveCount(0)
})
