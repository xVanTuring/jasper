import { test, expect } from '@playwright/test'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'
import { DATA_DIR } from './config'

// 标签视图（浏览）：侧栏标签区列出标签，点击按标签过滤笔记列表。
test('tag view: sidebar lists tags and filters notes by tag', async ({ page }) => {
	await openApp(page)

	// 侧栏标签区含 fixture 里的 'reading' 标签（篇数 1）
	const tagRow = page.locator('.tag-section button.row', { hasText: 'reading' })
	await expect(tagRow).toBeVisible()

	// 点击 → 笔记列表只剩打了该标签的 'Tagged Note'，列表标题为标签名
	await tagRow.click()
	await expect(page.locator('.notelist .pane-title')).toContainText('reading')
	await expect(page.locator('button.note', { hasText: 'Tagged Note' })).toBeVisible()
	await expect(page.locator('button.note', { hasText: 'Plain Note' })).toHaveCount(0)
})

// 打标签（读写，Joplin 兼容）：给笔记加标签 → chip/侧栏/API/落盘更新；去标签 → 还原。
test('tagging: add and remove a tag on a note (persists, Joplin-compatible on disk)', async ({
	page,
	request,
}) => {
	await openApp(page)

	// 打开预打标签的笔记，标签行应显示已有的 'reading' chip
	await page.locator('button.note', { hasText: 'Tagged Note' }).click()
	await expect(page.locator('.note-tags')).toBeVisible()
	await expect(page.locator('.note-tags .chip', { hasText: 'reading' })).toBeVisible()

	// 添加新标签 'focus'（回车提交）
	const input = page.locator('.note-tags input.add')
	await input.fill('focus')
	const addResp = page.waitForResponse(
		(r) => r.url().includes(`/api/notes/${IDS.tagNote}/tags`) && r.request().method() === 'POST' && r.ok(),
	)
	await input.press('Enter')
	await addResp

	// UI：新 chip 出现；侧栏标签区也出现 'focus'
	await expect(page.locator('.note-tags .chip', { hasText: 'focus' })).toBeVisible()
	await expect(page.locator('.tag-section button.row', { hasText: 'focus' })).toBeVisible()

	// API：该笔记标签含 reading + focus
	const apiTags = await (await request.get(`/api/notes/${IDS.tagNote}/tags`)).json()
	expect(apiTags.map((t: { title: string }) => t.title).sort()).toEqual(['focus', 'reading'])

	// 落盘兼容 Joplin：新写的 note_tag 是纯元数据(type_=5/6)、引用本笔记；新标签文件首行是标题
	const files = readdirSync(DATA_DIR).filter((f) => f.endsWith('.md'))
	let sawNewNoteTag = false
	let sawTagFile = false
	for (const f of files) {
		const c = readFileSync(join(DATA_DIR, f), 'utf8')
		if (c.includes('\ntype_: 6') && c.includes(`note_id: ${IDS.tagNote}`) && !f.startsWith(IDS.noteTag)) {
			sawNewNoteTag = true
			expect(c.startsWith('id: ')).toBeTruthy() // 无标题
			expect(c).toContain('type_: 6')
		}
		if (c.startsWith('focus\n\n') && c.includes('\ntype_: 5')) sawTagFile = true
	}
	expect(sawNewNoteTag).toBeTruthy()
	expect(sawTagFile).toBeTruthy()

	// 去掉 'focus'：点 chip 的 × → chip 消失，API 只剩 reading
	const focusChip = page.locator('.note-tags .chip', { hasText: 'focus' })
	const delResp = page.waitForResponse(
		(r) => r.url().includes(`/api/notes/${IDS.tagNote}/tags/`) && r.request().method() === 'DELETE' && r.ok(),
	)
	await focusChip.locator('.chip-x').click()
	await delResp
	await expect(page.locator('.note-tags .chip', { hasText: 'focus' })).toHaveCount(0)
	await expect(page.locator('.note-tags .chip', { hasText: 'reading' })).toBeVisible()

	const after = await (await request.get(`/api/notes/${IDS.tagNote}/tags`)).json()
	expect(after.map((t: { title: string }) => t.title)).toEqual(['reading'])
})
