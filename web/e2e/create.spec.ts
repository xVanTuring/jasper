import { test, expect } from '@playwright/test'
import { openApp } from './helpers'

// 新建笔记 → 出现在列表并进入编辑态。
test('create note adds it to the current notebook', async ({ page }) => {
	await openApp(page)
	// 首个笔记本已自动选中
	await expect(page.locator('button.folder', { hasText: 'Notebook' })).toBeVisible()

	await page.getByRole('button', { name: 'New note in this notebook' }).click()

	await expect(page.locator('button.note', { hasText: 'New note' })).toBeVisible()
	// 进入编辑态：标题输入框可见
	await expect(page.locator('input.title-input')).toBeVisible()
})
