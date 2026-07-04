// 编辑期钩子（spec §3.7，0.4 阶段 4）：page.route 伪造插件列表（含 contributes.editor on=input）
// 与 editor.transform 端点（返回全大写），断言源码编辑器**输入停顿后保守替换缓冲**。
// 隔离：拦掉目标笔记的 PUT（返回 ok 但不落盘），只验前端缓冲替换，避免污染其它 spec 的笔记内容。
import { test, expect, type Page } from '@playwright/test'
import { openApp } from './helpers'
import { IDS } from './make-fixture.mjs'

const PLUGIN_ID = 'e2e-editor'
const TARGET_NOTE = IDS.plainNote

const PLUGINS_RESP = {
	host: { version: '0.0.0-e2e', api_versions: ['0.1', '0.2', '0.3', '0.4'] },
	plugins: [
		{
			id: PLUGIN_ID,
			name: 'E2E Editor Hook',
			version: '1.0.0',
			api_version: '0.4',
			description: '',
			author: '',
			enabled: true,
			has_backend: true,
			capabilities: [],
			hooks: [],
			error: null,
			contributes: {
				theme: [],
				locale: [],
				storage: [],
				command: [],
				toolbar: [],
				sidebar: [],
				editor: [{ on: 'input' }],
			},
			settings_schema: {},
			write_auto_approve: false,
		},
	],
}

async function fakeRoutes(page: Page) {
	await page.route('**/api/plugins', (route) =>
		route.fulfill({ contentType: 'application/json', body: JSON.stringify(PLUGINS_RESP) }),
	)
	// editor.transform = 全大写整段（对齐 testbed 夹具语义，但这里不带相位前缀，方便断言）
	await page.route(`**/api/plugins/${PLUGIN_ID}/editor/transform`, async (route) => {
		const { text } = route.request().postDataJSON() as { phase: string; text: string }
		await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ text: text.toUpperCase() }) })
	})
	// 拦掉目标笔记的自动保存 PUT：回 ok 但不落盘（隔离，避免大写内容污染 edit/search 等 spec）
	await page.route(`**/api/notes/${TARGET_NOTE}`, async (route) => {
		if (route.request().method() !== 'PUT') return route.continue()
		const { title, body } = route.request().postDataJSON() as { title: string; body: string }
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({
				id: TARGET_NOTE,
				parent_id: IDS.notebook,
				title,
				body,
				markup_language: 1,
				is_todo: false,
				todo_completed: false,
			}),
		})
	})
}

test('input-phase editor plugin rewrites the source buffer after typing settles', async ({ page }) => {
	await fakeRoutes(page)
	await openApp(page, { editor: 'source' })

	await page.locator('button.note', { hasText: 'Plain Note' }).click()
	await page.getByRole('button', { name: 'Edit' }).click()

	const cm = page.locator('.cm-content')
	await expect(cm).toBeVisible()
	await cm.click()
	await page.keyboard.press('End')

	// 打字 → debounce 停顿后触发 editor.transform（等其响应），缓冲被替换为全大写
	const [req] = await Promise.all([
		page.waitForResponse((r) => r.url().includes(`/api/plugins/${PLUGIN_ID}/editor/transform`) && r.ok()),
		page.keyboard.type(' hello world'),
	])
	const sent = req.request().postDataJSON() as { phase: string; text: string }
	expect(sent.phase).toBe('input')

	// 断言编辑缓冲已被保守替换为全大写（含原文 + 刚输入）
	await expect(cm).toContainText('JUST SOME')
	await expect(cm).toContainText('HELLO WORLD')
	// 小写原样不应再出现（整段已大写）
	await expect(cm).not.toContainText('hello world')
})
