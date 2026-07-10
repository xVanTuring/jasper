import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'
import { DATA_DIR } from './config'

// 源码模式（CodeMirror）编辑 → 防抖自动保存 → 落盘 + API 可见。
test('source-mode edit persists to disk and API', async ({ page, request }) => {
	await openApp(page, { editor: 'source' })
	// 打开笔记默认即进编辑态（源码引擎，因 editor:'source'），无需再点「Edit」
	await page.locator('button.note', { hasText: 'Plain Note' }).click()

	const cm = page.locator('.cm-content')
	await expect(cm).toBeVisible()
	await cm.click()
	await page.keyboard.press('End')
	await page.keyboard.type(' EDITED-BY-E2E')

	await page.waitForResponse(
		(r) =>
			r.url().includes(`/api/notes/${IDS.plainNote}`) &&
			r.request().method() === 'PUT' &&
			r.ok(),
	)

	const detail = await (await request.get(`/api/notes/${IDS.plainNote}`)).json()
	expect(detail.body).toContain('EDITED-BY-E2E')

	const onDisk = readFileSync(join(DATA_DIR, `${IDS.plainNote}.md`), 'utf8')
	expect(onDisk).toContain('EDITED-BY-E2E')
})
