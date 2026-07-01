// 生成 README 用的预览图：中/英各一套，UI 语言与笔记内容都随之切换。
// 对每种语言：生成演示库 → 起真正的 Rust 后端 → Playwright 打开前端 → 截 4 张图。
// 产物写到 docs/screenshots/：英文用基名（01-reading.png…，供英文 README.md），
// 中文用 .zh 后缀（01-reading.zh.png…，供 README.zh-CN.md）。
//
// 前置：先 `cd web && pnpm build`（要 web/dist）+ `cd server && cargo build`（要 debug 二进制）。
// 运行：`cd web && node scripts/shoot.mjs`
import { chromium } from '@playwright/test'
import { spawn } from 'node:child_process'
import { existsSync, rmSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { makeDemoLibrary } from './demo-library.mjs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const WEB = resolve(__dirname, '..') // web/
const REPO = resolve(WEB, '..') // 仓库根
const SERVER_BIN = process.env.JASPER_BIN ?? resolve(REPO, 'server/target/debug/jasper')
const WEB_DIR = resolve(WEB, 'dist')
const OUT_DIR = resolve(REPO, 'docs/screenshots')

// 1280x820 @2x = 2560x1640，与既有截图尺寸一致。
const VIEWPORT = { width: 1280, height: 820 }
const SCALE = 2

const LOCALES = [
	{
		code: 'en',
		port: 27590,
		suffix: '',
		edit: 'Edit',
		read: 'Read',
		resources: 'Resources',
		techFolder: 'Tech Notes',
		showcase: 'Feature Showcase',
	},
	{
		code: 'zh',
		port: 27591,
		suffix: '.zh',
		edit: '编辑',
		read: '阅读',
		resources: '资源管理',
		techFolder: '技术笔记 · Tech',
		showcase: '功能展示',
	},
]

function fail(msg) {
	console.error(`\n[shoot] ${msg}\n`)
	process.exit(1)
}
if (!existsSync(SERVER_BIN)) fail(`找不到后端二进制：${SERVER_BIN}\n先构建：cd server && cargo build`)
if (!existsSync(WEB_DIR)) fail(`找不到前端产物：${WEB_DIR}\n先构建：cd web && pnpm build`)

// 等后端 /api/status 就绪。node 全局 fetch 不走 HTTP_PROXY，直连 127.0.0.1 即可。
async function waitForServer(port, tries = 120) {
	const url = `http://127.0.0.1:${port}/api/status`
	for (let i = 0; i < tries; i++) {
		try {
			const r = await fetch(url)
			if (r.ok) return
		} catch {
			/* 还没起来 */
		}
		await new Promise((r) => setTimeout(r, 250))
	}
	throw new Error(`后端在 ${port} 未就绪`)
}

function startServer({ port }, dataDir, configDir) {
	return spawn(SERVER_BIN, [], {
		env: {
			...process.env,
			JASPER_SOURCE: dataDir,
			JASPER_CONFIG_DIR: configDir,
			JASPER_WEB_DIR: WEB_DIR,
			JASPER_HOST: '127.0.0.1',
			JASPER_PORT: String(port),
		},
		stdio: 'ignore',
	})
}

async function shootLocale(browser, loc) {
	const base = join(tmpdir(), `jasper-shot-${loc.code}`)
	const dataDir = join(base, 'data')
	const configDir = join(base, 'config')
	// 每次重建为初始状态（清旧数据 + 隔离配置），保证可复现
	rmSync(base, { recursive: true, force: true })
	makeDemoLibraryInto(dataDir, loc.code)

	const srv = startServer(loc, dataDir, configDir)
	try {
		await waitForServer(loc.port)

		const ctx = await browser.newContext({
			viewport: VIEWPORT,
			deviceScaleFactor: SCALE,
			colorScheme: 'light',
			baseURL: `http://127.0.0.1:${loc.port}`,
		})
		// 每次导航前置 localStorage：语言、源码编辑器、浅色主题（避免跟随系统变暗）。
		await ctx.addInitScript(
			([code]) => {
				localStorage.setItem('jasper.locale', code)
				localStorage.setItem('jasper.editor', 'source')
				localStorage.setItem('jasper.theme', 'light')
			},
			[loc.code],
		)

		const page = await ctx.newPage()
		await page.goto('/')

		// 库加载完成后，显式选中「技术笔记」笔记本 → 其中的 Feature Showcase 笔记
		// （默认自动选中的是别的笔记本，内容较平淡）。
		await page.locator('button.folder', { hasText: loc.techFolder }).click()
		const showcaseNote = page.locator('button.note', { hasText: loc.showcase })
		await showcaseNote.waitFor({ state: 'visible' })

		// —— 01 阅读视图 ——（选中 Feature Showcase 笔记，等架构图加载）
		await showcaseNote.click()
		await page.locator('.content').waitFor({ state: 'visible' })
		await waitImageLoaded(page)
		await page.locator('.katex').first().waitFor({ state: 'visible' }).catch(() => {})
		await settle(page)
		await shot(page, join(OUT_DIR, `01-reading${loc.suffix}.png`))

		// —— 02 编辑器 ——（进入源码编辑）
		await page.getByRole('button', { name: loc.edit, exact: true }).click()
		await page.locator('.cm-content').waitFor({ state: 'visible' })
		await settle(page)
		await shot(page, join(OUT_DIR, `02-editor${loc.suffix}.png`))
		// 退出编辑：编辑态下同一按钮名变成「阅读/Read」
		await page.getByRole('button', { name: loc.read, exact: true }).click()
		await page.locator('.content').waitFor({ state: 'visible' })

		// —— 03 资源面板 ——（阅读态下打开资源管理弹层）
		await page.getByRole('button', { name: loc.resources, exact: true }).click()
		await page.locator('.card').waitFor({ state: 'visible' })
		await waitImageLoaded(page, '.card img')
		await settle(page)
		await shot(page, join(OUT_DIR, `03-resources${loc.suffix}.png`))
		await page.keyboard.press('Escape')
		await page.locator('.card').waitFor({ state: 'hidden' })

		// —— 04 搜索 ——（右侧仍是 Showcase 笔记，左侧列表按 Rust 过滤）
		const box = page.getByRole('searchbox')
		await box.click()
		await box.fill('Rust')
		// 等列表过滤生效：非匹配的笔记（旅行碎记/Trip）消失
		await page.waitForFunction(() => {
			const titles = [...document.querySelectorAll('button.note')].map((b) => b.textContent || '')
			return titles.length > 0 && titles.length <= 4
		})
		await settle(page)
		await shot(page, join(OUT_DIR, `04-search${loc.suffix}.png`))

		await ctx.close()
		console.log(`[shoot] ${loc.code} ✓`)
	} finally {
		srv.kill('SIGTERM')
	}
}

// 单独封装，便于上面读起来顺；实际就是调 makeDemoLibrary。
function makeDemoLibraryInto(dir, code) {
	mkdirSync(dir, { recursive: true })
	makeDemoLibrary(dir, code)
}

async function waitImageLoaded(page, sel = '.content img') {
	await page
		.waitForFunction(
			(s) => {
				const img = document.querySelector(s)
				return !!img && img.complete && img.naturalWidth > 0
			},
			sel,
			{ timeout: 5000 },
		)
		.catch(() => {})
}

// 等字体/渲染稳定，避免半渲染出图。
async function settle(page) {
	await page.evaluate(() => document.fonts?.ready).catch(() => {})
	await page.waitForTimeout(250)
}

async function shot(page, path) {
	await page.mouse.move(2, 2) // 移开鼠标，避免列表项 hover 高亮
	await page.screenshot({ path, animations: 'disabled' })
}

async function main() {
	mkdirSync(OUT_DIR, { recursive: true })
	// 默认用 Playwright 自带 chromium；本地未下载时可设 SHOOT_CHANNEL=chrome 用系统 Chrome。
	const browser = await chromium.launch({
		channel: process.env.SHOOT_CHANNEL || undefined,
		args: ['--no-proxy-server'],
	})
	try {
		for (const loc of LOCALES) await shootLocale(browser, loc)
	} finally {
		await browser.close()
	}
	console.log('[shoot] done →', OUT_DIR)
}

main().catch((e) => {
	console.error(e)
	process.exit(1)
})
