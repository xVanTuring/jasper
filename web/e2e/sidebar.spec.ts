// 插件侧边栏（spec 0.3）：page.route 伪造插件列表 / ui 树 / 命令响应（无需真 wasm），
// 但**写提案的批准路径走真后端** PUT /api/notes/{id} —— 断言笔记真的落盘变更。
// 覆盖：左栏入口 → 右侧 dock → 静态 chat 往返 → 动态树按钮 → 写确认 同意/拒绝。
// 前置：后端以 --features plugins 构建；否则整组自动跳过。
import { test, expect, type Page } from '@playwright/test'
import { IDS } from './make-fixture.mjs'

// 提案目标用 todoNote —— 其它 spec（edit.spec 等）不碰它，避免全量跑时的顺序污染
const PLUGIN_ID = 'e2e-sidebar'
const TARGET_NOTE = IDS.todoNote
const REWRITTEN_BODY = 'plugin-rewritten body'

const PLUGINS_RESP = {
	host: { version: '0.0.0-e2e', api_versions: ['0.1', '0.2', '0.3'] },
	plugins: [
		{
			id: PLUGIN_ID,
			name: 'E2E Sidebar',
			version: '1.0.0',
			api_version: '0.3',
			description: '',
			author: '',
			enabled: true,
			has_backend: true,
			capabilities: ['notes:read', 'notes:write'],
			hooks: [],
			error: null,
			contributes: {
				theme: [],
				storage: [],
				command: [
					{ id: 'chat', title: 'Chat', icon: '', target: 'backend' },
					{ id: 'propose', title: 'Propose', icon: '', target: 'backend' },
				],
				toolbar: [],
				sidebar: [
					{ id: 'chat-panel', title: 'E2E Chat', icon: 'chat', widget: 'chat', command: 'chat' },
					{ id: 'tools', title: 'E2E Tools', icon: '', widget: 'markdown', view: 'main' },
				],
			},
			settings_schema: {},
			write_auto_approve: false,
		},
	],
}

const UI_TREE = {
	ui: {
		type: 'markdown',
		props: { source: '**tools panel**' },
		children: [{ type: 'button', props: { label: 'Rewrite', command: 'propose' } }],
	},
	pending_writes: [],
}

function pendingWrite() {
	return {
		action: 'update',
		plugin_id: PLUGIN_ID,
		note: { id: TARGET_NOTE, parent_id: IDS.notebook, title: 'Todo Note', body: REWRITTEN_BODY },
		original: { title: 'Todo Note', body: '- [ ] task one\n- [x] task two' },
	}
}

async function fakePluginRoutes(page: Page) {
	await page.route('**/api/plugins', (route) =>
		route.fulfill({ contentType: 'application/json', body: JSON.stringify(PLUGINS_RESP) }),
	)
	await page.route(`**/api/plugins/${PLUGIN_ID}/ui/main`, (route) =>
		route.fulfill({ contentType: 'application/json', body: JSON.stringify(UI_TREE) }),
	)
	await page.route(`**/api/plugins/${PLUGIN_ID}/commands/chat`, async (route) => {
		const body = route.request().postDataJSON() as { args: { input: string; note_id: string | null } }
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({
				result: { reply: `echo: ${body.args.input}` },
				pending_writes: [],
			}),
		})
	})
	await page.route(`**/api/plugins/${PLUGIN_ID}/commands/propose`, (route) =>
		route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({ result: {}, pending_writes: [pendingWrite()] }),
		}),
	)
}

async function noteBody(page: Page): Promise<string> {
	const detail = await (await page.request.get(`/api/notes/${TARGET_NOTE}`)).json()
	return detail.body as string
}

test.describe('插件侧边栏 + 写确认', () => {
	test.beforeEach(async ({ page }) => {
		const resp = await page.request.get('/api/plugins')
		const ct = resp.headers()['content-type'] ?? ''
		test.skip(!resp.ok() || !ct.includes('application/json'), '服务端未编译 --features plugins')
		await fakePluginRoutes(page)
		await page.goto('/')
	})

	test('左栏入口 → dock 打开 → 静态 chat 往返', async ({ page }) => {
		// 入口在左栏底部
		const entry = page.locator('.plugin-entry', { hasText: 'E2E Chat' })
		await expect(entry).toBeVisible()
		await entry.click()

		// dock 出现，发消息 → 伪造命令回显为 assistant 消息
		const dock = page.locator('.dock')
		await expect(dock).toBeVisible()
		await dock.locator('textarea').fill('你好插件')
		await dock.locator('textarea').press('Enter')
		await expect(dock.locator('.msg.user')).toContainText('你好插件')
		await expect(dock.locator('.msg.assistant')).toContainText('echo: 你好插件')

		// 再点入口收起
		await entry.click()
		await expect(dock).toHaveCount(0)
	})

	test('动态树渲染 → 按钮触发提案 → 同意走真 PUT 落盘', async ({ page }) => {
		const before = await noteBody(page)

		await page.locator('.plugin-entry', { hasText: 'E2E Tools' }).click()
		const dock = page.locator('.dock')
		await expect(dock.locator('strong', { hasText: 'tools panel' })).toBeVisible()

		// server-driven 按钮 → 命令响应携带写提案 → 确认框弹出并展示 diff
		await dock.getByRole('button', { name: 'Rewrite' }).click()
		const dialog = page.locator('.overlay', { hasText: /请求修改笔记|wants to modify/ })
		await expect(dialog).toBeVisible()
		await expect(dialog.locator('.line.add', { hasText: REWRITTEN_BODY })).toBeVisible()

		// 同意 → 真后端 PUT /api/notes/{id} 落盘
		await dialog.getByRole('button', { name: /^(应用|Apply)$/ }).click()
		await expect(dialog).toHaveCount(0)
		await expect.poll(() => noteBody(page)).toBe(REWRITTEN_BODY)

		// 复原（直接走真 API，避免污染其它用例）
		await page.request.put(`/api/notes/${TARGET_NOTE}`, {
			data: { title: 'Todo Note', body: before },
		})
	})

	test('拒绝提案 → 不落盘', async ({ page }) => {
		const before = await noteBody(page)

		await page.locator('.plugin-entry', { hasText: 'E2E Tools' }).click()
		const dock = page.locator('.dock')
		await dock.getByRole('button', { name: 'Rewrite' }).click()

		const dialog = page.locator('.overlay', { hasText: /请求修改笔记|wants to modify/ })
		await expect(dialog).toBeVisible()
		await dialog.getByRole('button', { name: /^(拒绝|Reject)$/ }).click()
		await expect(dialog).toHaveCount(0)

		expect(await noteBody(page)).toBe(before)
	})
})
