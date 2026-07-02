import { test, expect } from '@playwright/test'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'

// SSE 自动刷新（/api/events）：外部写入（curl / 插件免确认直写同路径）不经手动刷新
// 即反映到打开中的页面。回显守 design doc §5.3 保守规则——本测试里用户未在输入，应被替换。
test('external note edit auto-refreshes the open note and the list', async ({ page, request }) => {
	await openApp(page, { editor: 'source' })
	await page.locator('button.note', { hasText: 'Plain Note' }).click()
	await expect(page.locator('main.reader')).toContainText('Just some')

	// 模拟外部写入（与插件免确认直写共用 persist_note_blocking → 同一事件源）
	const detail = await (await request.get(`/api/notes/${IDS.plainNote}`)).json()
	const put = await request.put(`/api/notes/${IDS.plainNote}`, {
		data: { title: 'SSE Updated', body: `${detail.body}\n\nSSE-EDIT-MARKER` },
	})
	expect(put.ok()).toBe(true)

	// 不做任何页面交互：SSE → 去抖合并刷新 → 阅读视图与列表自动更新
	await expect(page.locator('main.reader')).toContainText('SSE-EDIT-MARKER', { timeout: 5000 })
	await expect(page.locator('button.note', { hasText: 'SSE Updated' })).toBeVisible({ timeout: 5000 })
})

// 外部新建：列表与笔记本计数自动出现（unknown id → folders+list 全刷）。
// 注意：同一 webServer 贯穿本文件（串行、有状态），不要依赖上个测试改过的标题。
test('externally created note shows up in the open folder', async ({ page, request }) => {
	await openApp(page)
	await expect(page.locator('button.note').first()).toBeVisible() // 默认笔记本已选中、列表就绪

	const post = await request.post('/api/notes', {
		data: { parent_id: IDS.notebook, title: 'Born Outside', body: 'created via API' },
	})
	expect(post.ok()).toBe(true)

	await expect(page.locator('button.note', { hasText: 'Born Outside' })).toBeVisible({ timeout: 5000 })
})
