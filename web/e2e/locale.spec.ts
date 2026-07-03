// 语言包插件全链路：安装零代码语言包 → 自动启用 → LangPicker 出现「Français」→ 选中后
// 顶栏搜索占位符切换为法语（catalog 命中的 key）→ 卸载后语言回落、占位符复原。
// 前置：后端以 --features plugins 构建；否则整组自动跳过。
import { test, expect, type Page } from '@playwright/test'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const FIXTURES = path.join(path.dirname(fileURLToPath(import.meta.url)), 'fixtures')
const FR_SEARCH = 'Rechercher des notes…'

async function openPluginPanel(page: Page) {
	await page.getByRole('button', { name: /^(插件|Plugins)$/ }).click()
}

async function installFixture(page: Page, file: string) {
	const chooser = page.waitForEvent('filechooser')
	await page.getByRole('button', { name: /安装插件|Install plugin/ }).click()
	await (await chooser).setFiles(path.join(FIXTURES, file))
}

test.describe('语言包插件', () => {
	test.beforeEach(async ({ page }) => {
		const resp = await page.request.get('/api/plugins')
		const ct = resp.headers()['content-type'] ?? ''
		test.skip(!resp.ok() || !ct.includes('application/json'), '服务端未编译 --features plugins')
		await page.goto('/')
	})

	test('安装→自动启用→LangPicker 出现法语→选中生效→卸载回落', async ({ page }) => {
		const search = page.locator('input.search')
		await expect(search).not.toHaveAttribute('placeholder', FR_SEARCH) // 初始非法语

		await openPluginPanel(page)
		await installFixture(page, 'locale.jplug')

		// 行出现且自动启用（零代码信任档）
		const row = page.locator('.row', { hasText: 'E2E Français' })
		await expect(row).toBeVisible()
		await expect(row.locator('.switch input')).toBeChecked()

		// 关面板 → 语言选择器里出现「Français」并可选中
		await page.keyboard.press('Escape')
		await page.getByRole('button', { name: /切换语言|Switch language/ }).click()
		await page.getByRole('menuitemradio', { name: 'Français' }).click()

		// catalog 命中的 key → 顶栏搜索占位符切法语（未翻的 key 自动回落 base=en）
		await expect(search).toHaveAttribute('placeholder', FR_SEARCH)

		// 卸载 → 语言来源消失 → 回落内置语言，占位符复原
		page.on('dialog', (d) => d.accept())
		await openPluginPanel(page)
		await row.getByRole('button', { name: /卸载|Uninstall/ }).click()
		await expect(row).toHaveCount(0)
		await expect(search).not.toHaveAttribute('placeholder', FR_SEARCH)
		// 选择器里不再有法语
		await page.keyboard.press('Escape')
		await page.getByRole('button', { name: /切换语言|Switch language/ }).click()
		await expect(page.getByRole('menuitemradio', { name: 'Français' })).toHaveCount(0)
	})
})
