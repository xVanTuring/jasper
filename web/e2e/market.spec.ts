// 插件市场全链路：page.route 伪造 registry 索引与下载 URL（真实 .jplug 夹具字节 + 真 sha256）
// → 市场 tab 浏览（双语索引取词 / 不兼容置灰）→ 安装（浏览器下载 + sha256 校验）→ 自动切回已安装
// → sha256 不匹配的条目安装被中止并报错。
// 前置：后端以 --features plugins 构建；否则整组自动跳过。
import { test, expect, type Page } from '@playwright/test'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const FIXTURES = path.join(path.dirname(fileURLToPath(import.meta.url)), 'fixtures')
const INDEX_URL = 'https://raw.githubusercontent.com/xVanTuring/jasper-plugin-registry/main/plugins.json'

const themeBytes = readFileSync(path.join(FIXTURES, 'theme.jplug'))
const themeSha = createHash('sha256').update(themeBytes).digest('hex')

// 索引：1 个可装（sha 正确，version/apiVersion 对齐夹具 manifest）、
// 1 个 sha 被篡改、1 个 apiVersion 大版本不兼容。
const INDEX = {
	schemaVersion: 1,
	plugins: [
		{
			id: 'e2e-theme',
			name: { zh: '端到端主题', en: 'E2E Market Theme' },
			description: { zh: '来自市场的测试主题', en: 'A test theme served from the market' },
			author: 'e2e',
			repo: 'https://example.com/repo',
			version: '1.0.0',
			apiVersion: '0.1',
			minHostVersion: '',
			capabilities: [],
			download: 'https://plugins.example/e2e-theme.jplug',
			sha256: themeSha,
		},
		{
			id: 'bad-sha',
			name: { zh: '坏校验', en: 'Bad Checksum' },
			description: { zh: '索引 sha256 与产物不符', en: 'Index sha256 does not match the artifact' },
			author: 'e2e',
			repo: '',
			version: '1.0.0',
			apiVersion: '0.1',
			minHostVersion: '',
			capabilities: [],
			download: 'https://plugins.example/bad-sha.jplug',
			sha256: '0'.repeat(64),
		},
		{
			id: 'future-plugin',
			name: { zh: '未来插件', en: 'Future Plugin' },
			description: { zh: '需要下一代插件 API', en: 'Needs a next-gen plugin API' },
			author: 'e2e',
			repo: '',
			version: '9.0.0',
			apiVersion: '1.0',
			minHostVersion: '',
			capabilities: [],
			download: 'https://plugins.example/future.jplug',
			sha256: themeSha,
		},
	],
}

async function openMarketTab(page: Page) {
	await page.getByRole('button', { name: /^(插件|Plugins)$/ }).click()
	await page.getByRole('tab', { name: /市场|Market/ }).click()
}

test.describe('插件市场', () => {
	test.beforeEach(async ({ page }) => {
		const resp = await page.request.get('/api/plugins')
		const ct = resp.headers()['content-type'] ?? ''
		test.skip(!resp.ok() || !ct.includes('application/json'), '服务端未编译 --features plugins')
		await page.route(INDEX_URL, (route) =>
			route.fulfill({ contentType: 'application/json', body: JSON.stringify(INDEX) }),
		)
		await page.route('https://plugins.example/**', (route) =>
			route.fulfill({ contentType: 'application/zip', body: themeBytes }),
		)
		await page.goto('/')
	})

	test('浏览→安装→已装状态；坏 sha256 中止；不兼容置灰', async ({ page }) => {
		await openMarketTab(page)

		// 双语索引按当前语言取词（en 环境默认英文名）
		const row = page.locator('.row', { hasText: /端到端主题|E2E Market Theme/ })
		await expect(row).toBeVisible()

		// 不兼容条目：无安装按钮，显示原因
		const future = page.locator('.row', { hasText: /未来插件|Future Plugin/ })
		await expect(future.getByText(/需要更新版本|Needs a newer Jasper/)).toBeVisible()
		await expect(future.getByRole('button')).toHaveCount(0)

		// 安装：下载→sha256 校验→装进宿主→自动切回「已安装」tab（零代码主题自动启用）
		await row.getByRole('button', { name: /安装|Install/ }).click()
		const installedRow = page.locator('.row', { hasText: 'E2E Theme' })
		await expect(installedRow).toBeVisible()
		await expect(installedRow.locator('.switch input')).toBeChecked()

		// 回到市场 → 该条目显示已安装（version 相同，无更新按钮）
		await page.getByRole('tab', { name: /市场|Market/ }).click()
		await expect(row.getByText(/已安装|Installed/)).toBeVisible()
		await expect(row.getByRole('button')).toHaveCount(0)

		// 坏 sha256：下载成功但校验失败 → 报错并中止（宿主没收到安装请求 → 已安装列表无此项）
		const bad = page.locator('.row', { hasText: /坏校验|Bad Checksum/ })
		await bad.getByRole('button', { name: /安装|Install/ }).click()
		await expect(page.getByText(/下载校验失败|Download verification failed/)).toBeVisible()

		// 清理：卸载，避免影响其它用例
		page.on('dialog', (d) => d.accept())
		await page.getByRole('tab', { name: /已安装|Installed/ }).click()
		await installedRow.getByRole('button', { name: /卸载|Uninstall/ }).click()
		await expect(installedRow).toHaveCount(0)
	})
})
