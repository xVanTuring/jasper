import { test, expect } from '@playwright/test'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'
import { DATA_DIR } from './config'

// 核心回归：富文本(Crepe)编辑保存后，图片说明 alt 不应被写成缩放比例。
// 见 web/src/lib/milkdown/imageBlockAlt.ts 的修复。
test('WYSIWYG edit keeps image alt as the description, not the ratio', async ({ page, request }) => {
	const ref = `![说明](:/${IDS.resource})`

	await openApp(page, { editor: 'wysiwyg' })
	await page.locator('button.note', { hasText: 'Image Note' }).click()
	await page.getByRole('button', { name: 'Edit' }).click()

	// Crepe 懒加载完成
	const pm = page.locator('.milkdown .ProseMirror')
	await expect(pm).toBeVisible()
	// 图片块渲染出来（:/id 已被 proxyDomURL 解析成真实资源地址）
	await expect(page.locator('.milkdown img')).toHaveAttribute(
		'src',
		new RegExp(`/api/resources/${IDS.resource}$`),
	)

	// 在正文段落里输入，触发一次经序列化器的自动保存（会整篇重排，含图片块）
	await page.getByText('Intro line.').click()
	await page.keyboard.press('End')
	await page.keyboard.type(' ZZZ')

	const put = await page.waitForResponse(
		(r) =>
			r.url().includes(`/api/notes/${IDS.imageNote}`) &&
			r.request().method() === 'PUT' &&
			r.ok(),
	)

	// 发出的保存请求体：确经富文本序列化器重排（编辑文本在内），且 alt 仍是「说明」而非 1.00
	const sent = put.request().postDataJSON() as { body: string }
	expect(sent.body).toContain('ZZZ') // 证明 WYSIWYG 序列化器确实跑过整篇
	expect(sent.body).toContain(ref)
	expect(sent.body).not.toMatch(/!\[\d+\.\d+\]/)

	// 落盘 + 再取详情都应保留原语义
	const detail = await (await request.get(`/api/notes/${IDS.imageNote}`)).json()
	expect(detail.body).toContain(ref)

	const onDisk = readFileSync(join(DATA_DIR, `${IDS.imageNote}.md`), 'utf8')
	expect(onDisk).toContain(ref)
	expect(onDisk).not.toMatch(/!\[\d+\.\d+\]/)
})
