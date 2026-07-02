// 插件管理全链路：安装主题包 → 零代码自动启用 → ThemePicker 出现并生效 → 卸载回落；
// 含 [backend] 的包 → consent 弹窗（联网警告）→ 保持禁用。
// 前置：后端以 --features plugins 构建；否则整组自动跳过。
import { test, expect, type Page } from '@playwright/test'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const FIXTURES = path.join(path.dirname(fileURLToPath(import.meta.url)), 'fixtures')

async function openPluginPanel(page: Page) {
	await page.getByRole('button', { name: /^(插件|Plugins)$/ }).click()
}

async function installFixture(page: Page, file: string) {
	const chooser = page.waitForEvent('filechooser')
	await page.getByRole('button', { name: /安装插件|Install plugin/ }).click()
	await (await chooser).setFiles(path.join(FIXTURES, file))
}

test.describe('插件管理', () => {
	test.beforeEach(async ({ page }) => {
		const resp = await page.request.get('/api/plugins')
		const ct = resp.headers()['content-type'] ?? ''
		test.skip(!resp.ok() || !ct.includes('application/json'), '服务端未编译 --features plugins')
		await page.goto('/')
	})

	test('主题插件：安装→自动启用→选中生效→卸载回落', async ({ page }) => {
		await openPluginPanel(page)
		await installFixture(page, 'theme.jplug')

		// 行出现且自动启用（零代码信任档）
		const row = page.locator('.row', { hasText: 'E2E Theme' })
		await expect(row).toBeVisible()
		await expect(row.locator('.switch input')).toBeChecked()
		// 主题 CSS 注入
		await expect(page.locator('link#plugin-theme-e2e-theme-e2e-lime')).toHaveCount(1)

		// 关面板 → ThemePicker 里出现并可选中
		await page.keyboard.press('Escape')
		await page.getByRole('button', { name: /主题|Theme/ }).click()
		await page.getByRole('menuitemradio', { name: 'E2E Lime' }).click()
		await expect(page.locator('html')).toHaveAttribute('data-theme', 'e2e-lime')

		// 卸载 → 主题回落（不再是插件主题）
		page.on('dialog', (d) => d.accept())
		await openPluginPanel(page)
		await row.getByRole('button', { name: /卸载|Uninstall/ }).click()
		await expect(row).toHaveCount(0)
		await expect(page.locator('link#plugin-theme-e2e-theme-e2e-lime')).toHaveCount(0)
		await expect(page.locator('html')).not.toHaveAttribute('data-theme', 'e2e-lime')
	})

	test('后端插件：consent 弹窗列能力含联网警告，保持禁用', async ({ page }) => {
		await openPluginPanel(page)
		await installFixture(page, 'caps-demo.jplug')

		// 含 backend → needs_consent，安装后立即弹授权（能力清单 + host:http 显式联网警告）
		await expect(page.getByText(/任意网址|any URL/)).toBeVisible()
		await expect(page.getByText(/读取笔记|Read notes/)).toBeVisible()

		// 保持禁用 → 弹窗关、行在场且开关关（enable = 授权动作未发生）
		await page.getByRole('button', { name: /保持禁用|Keep disabled/ }).click()
		await expect(page.getByText(/任意网址|any URL/)).toHaveCount(0)
		const row = page.locator('.row', { hasText: 'Caps Demo' })
		await expect(row).toBeVisible()
		await expect(row.locator('.switch input')).not.toBeChecked()

		// 再点启用 → 同一弹窗再次出现（授权始终前置）→ 再次保持禁用
		await row.locator('.switch').click()
		await expect(page.getByText(/任意网址|any URL/)).toBeVisible()
		await page.getByRole('button', { name: /保持禁用|Keep disabled/ }).click()
		await expect(row.locator('.switch input')).not.toBeChecked()
	})
})
