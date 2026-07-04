// AI 对话增强（2026-07-04）：page.route 伪造 chat 插件 + chat 命令端点，验证
// ① 笔记内容区的选区随命令 args.selection 传给插件；② 会话历史 localStorage 持久化（关闭 dock 再开续上）。
// 前置：后端 --features plugins 构建；否则整组跳过。
import { test, expect, type Page } from '@playwright/test'

const PLUGIN_ID = 'e2e-chat'

const PLUGINS_RESP = {
	host: { version: '0.0.0-e2e', api_versions: ['0.1', '0.2', '0.3', '0.4'] },
	plugins: [
		{
			id: PLUGIN_ID,
			name: 'E2E Chat',
			version: '1.0.0',
			api_version: '0.3',
			description: '',
			author: '',
			enabled: true,
			has_backend: true,
			capabilities: ['host:ai'],
			hooks: [],
			error: null,
			contributes: {
				theme: [],
				locale: [],
				storage: [],
				command: [{ id: 'chat', title: 'Chat', icon: '', target: 'backend' }],
				toolbar: [],
				sidebar: [{ id: 'chat-panel', title: 'E2E Chat', icon: 'chat', widget: 'chat', command: 'chat' }],
			},
			settings_schema: {},
			write_auto_approve: false,
		},
	],
}

test.describe('AI 对话：选区上下文 + 会话持久化', () => {
	test.beforeEach(async ({ page }) => {
		const resp = await page.request.get('/api/plugins')
		const ct = resp.headers()['content-type'] ?? ''
		test.skip(!resp.ok() || !ct.includes('application/json'), '服务端未编译 --features plugins')
		await page.route('**/api/plugins', (route) =>
			route.fulfill({ contentType: 'application/json', body: JSON.stringify(PLUGINS_RESP) }),
		)
		await page.goto('/')
	})

	test('选区随命令 args.selection 传入；发送后回复追加', async ({ page }) => {
		let lastArgs: Record<string, unknown> | null = null
		await page.route(`**/api/plugins/${PLUGIN_ID}/commands/chat`, async (route) => {
			lastArgs = (route.request().postDataJSON() as { args: Record<string, unknown> }).args
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({ result: { reply: '收到你的选区了' }, pending_writes: [] }),
			})
		})

		// 打开一篇笔记（阅读视图）
		await page.locator('button.note', { hasText: 'Plain Note' }).click()
		await expect(page.locator('.content')).toBeVisible()

		// 打开 chat dock
		await page.locator('.plugin-entry', { hasText: 'E2E Chat' }).click()
		const dock = page.locator('.dock')
		await expect(dock).toBeVisible()

		// 在笔记内容区选中文字 → composer 出现「已选中」chip（证明选区被捕获）
		await page.locator('.content').selectText()
		await expect(dock.locator('.sel-chip')).toBeVisible()

		// 发送 → 伪造命令回显
		await dock.locator('textarea').fill('优化这段')
		await dock.locator('textarea').press('Enter')
		await expect(dock.locator('.msg.assistant')).toContainText('收到你的选区了')

		// 断言：命令收到了 selection.text（含笔记正文文字）
		expect(lastArgs).not.toBeNull()
		const sel = (lastArgs as { selection?: { text?: string } }).selection
		expect(sel?.text).toBeTruthy()
		expect(sel?.text).toContain('markdown')
	})

	test('会话历史持久化：关闭 dock 再打开，消息续上', async ({ page }) => {
		await page.route(`**/api/plugins/${PLUGIN_ID}/commands/chat`, (route) =>
			route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({ result: { reply: '记住了' }, pending_writes: [] }),
			}),
		)
		const entry = page.locator('.plugin-entry', { hasText: 'E2E Chat' })
		await entry.click()
		const dock = page.locator('.dock')
		await dock.locator('textarea').fill('这是一条要记住的消息')
		await dock.locator('textarea').press('Enter')
		await expect(dock.locator('.msg.assistant')).toContainText('记住了')

		// 收起 dock，再打开 → 上一轮对话仍在（localStorage 持久化）
		await entry.click()
		await expect(dock).toHaveCount(0)
		await entry.click()
		await expect(page.locator('.dock')).toBeVisible()
		await expect(page.locator('.dock .msg.user')).toContainText('这是一条要记住的消息')
		await expect(page.locator('.dock .msg.assistant')).toContainText('记住了')
	})

	test('新建会话清空当前对话', async ({ page }) => {
		await page.route(`**/api/plugins/${PLUGIN_ID}/commands/chat`, (route) =>
			route.fulfill({ contentType: 'application/json', body: JSON.stringify({ result: { reply: 'ok' }, pending_writes: [] }) }),
		)
		await page.locator('.plugin-entry', { hasText: 'E2E Chat' }).click()
		const dock = page.locator('.dock')
		await dock.locator('textarea').fill('第一条')
		await dock.locator('textarea').press('Enter')
		await expect(dock.locator('.msg.user')).toContainText('第一条')

		// 点「新建会话」（+）→ 消息区清空
		await dock.getByRole('button', { name: 'New session' }).click()
		await expect(dock.locator('.msg')).toHaveCount(0)
	})
})
