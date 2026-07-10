import { test, expect } from '@playwright/test'
import { openApp, openNoteRead } from './helpers'

test('loads the library: notebook and its notes', async ({ page }) => {
	await openApp(page)
	await expect(page.locator('button.folder', { hasText: 'Notebook' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Image Note' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Todo Note' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Plain Note' })).toBeVisible()
})

test('renders a note and resolves :/id images to the resource API', async ({ page }) => {
	await openApp(page)
	await openNoteRead(page, 'Image Note')

	const img = page.locator('.content img')
	await expect(img).toHaveAttribute('src', /\/api\/resources\/5{32}$/)
	// 图片确实加载成功（proxyDomURL / 资源 API 都通）
	await expect
		.poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth))
		.toBeGreaterThan(0)
})

test('todo note shows task-list progress', async ({ page }) => {
	await openApp(page)
	await openNoteRead(page, 'Todo Note')
	await expect(page.getByText('Tasks 1/2').first()).toBeVisible()
})

test('renders markdown formatting', async ({ page }) => {
	await openApp(page)
	await openNoteRead(page, 'Plain Note')
	await expect(page.locator('.content strong', { hasText: 'markdown' })).toBeVisible()
})
