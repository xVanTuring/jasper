// 设置面板 × 插件数据源：page.route 伪造 provider（无需真插件/wasm），
// 断言侧边栏「数据源」分区里的动态 provider 选项 + SchemaForm 渲染 + PUT /api/config 的 payload 形状。
// 数据源描述符由真后端 /api/settings/schema 提供（默认构建，无 AI 段）；provider 选项经 options_source
// 合入伪造的 /api/plugins storage 贡献。
import { test, expect } from '@playwright/test'

const FAKE_PLUGINS = {
	host: { version: '0.0.0-e2e', api_versions: ['0.1', '0.2'] },
	plugins: [
		{
			id: 'demo-cloud',
			name: 'Demo Cloud',
			version: '1.0.0',
			api_version: '0.2',
			description: '',
			author: '',
			enabled: true,
			has_backend: true,
			capabilities: ['host:http'],
			hooks: [],
			error: null,
			contributes: {
				theme: [],
				locale: [],
				storage: [
					{
						id: 'cloud',
						name: 'Demo Cloud',
						icon: '',
						config_schema: {
							url: { type: 'string', label: '服务地址', required: true, placeholder: 'https://…' },
							token: { type: 'secret', label: 'Token' },
						},
					},
				],
			},
			settings_schema: {},
		},
	],
}

test('向导出现插件 provider，动态表单提交正确 payload', async ({ page }) => {
	await page.route('**/api/plugins', (route) =>
		route.fulfill({ json: FAKE_PLUGINS, contentType: 'application/json' }),
	)
	let captured: Record<string, unknown> | null = null
	await page.route('**/api/config', async (route) => {
		if (route.request().method() === 'PUT') {
			captured = route.request().postDataJSON() as Record<string, unknown>
			await route.fulfill({ json: { ok: true, error: null, notes: 1, folders: 1 } })
		} else {
			await route.fallback()
		}
	})

	await page.goto('/')
	await page.getByRole('button', { name: /^(设置|Settings)$/ }).click()

	// 侧边栏导航到「数据源」分区（默认即选中，此步显式确认新结构且抗分区重排）
	await page.getByRole('button', { name: /^(数据源|Data source)$/ }).click()

	// 动态 provider 选项按钮（source_type enum 经 options_source 合入伪造的 contributes.storage）
	const providerBtn = page.getByRole('button', { name: 'Demo Cloud' })
	await expect(providerBtn).toBeVisible()
	await providerBtn.click()

	// SchemaForm：必填校验先拦下
	await page.getByRole('button', { name: /^(连接|Connect)$/ }).click()
	await expect(page.getByText(/必填|Required/)).toBeVisible()
	expect(captured).toBeNull()

	// 填表提交（限定在 provider 子表单 .plugin-form 内）
	await page.getByPlaceholder('https://…').fill('https://cloud.example/dav/')
	await page.locator('.plugin-form input[type="password"]').fill('tok-123')
	await page.getByRole('button', { name: /^(连接|Connect)$/ }).click()

	await expect.poll(() => captured).not.toBeNull()
	expect(captured).toMatchObject({
		source_type: 'plugin',
		plugin_id: 'demo-cloud',
		plugin_storage: 'cloud',
		plugin_config: { url: 'https://cloud.example/dav/', token: 'tok-123' },
	})
})
