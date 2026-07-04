// 生成 README 用的「访问控制生效」截图：设了访问密码、未登录访问时的登录闸门。
// 中/英各一张，写到 docs/screenshots/06-access-control(.zh).png。
//
// 前置：先 `cd web && pnpm build`（要 web/dist）+ `cd server && cargo build --features plugins`（要 debug 二进制）。
// 运行：`cd web && node scripts/shoot-auth.mjs`
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

// 与 shoot.mjs 一致的画布尺寸，保证和其余截图并排时观感统一。
const VIEWPORT = { width: 1280, height: 820 }
const SCALE = 2
const DEMO_PASSWORD = 'jasper-demo'

const LOCALES = [
	{ code: 'en', port: 27592, suffix: '' },
	{ code: 'zh', port: 27593, suffix: '.zh' },
]

function fail(msg) {
	console.error(`\n[shoot-auth] ${msg}\n`)
	process.exit(1)
}
if (!existsSync(SERVER_BIN)) fail(`找不到后端二进制：${SERVER_BIN}\n先构建：cd server && cargo build`)
if (!existsSync(WEB_DIR)) fail(`找不到前端产物：${WEB_DIR}\n先构建：cd web && pnpm build`)

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

function startServer(port, dataDir, configDir) {
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

// 起服务后直接调 PUT /api/auth/settings 设一个访问密码：此时尚无密码，Access 恒 Full，不需要 token。
// passwordless_read=false → 整站私有，未登录访客撞上登录闸门（而非仍可只读浏览）。
async function setPassword(port) {
	const r = await fetch(`http://127.0.0.1:${port}/api/auth/settings`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			password: DEMO_PASSWORD,
			passwordless_read: false,
			list_mode: 'none',
			folder_list: [],
		}),
	})
	if (!r.ok) throw new Error(`设置访问密码失败：HTTP ${r.status}`)
}

async function shootLocale(browser, loc) {
	const base = join(tmpdir(), `jasper-shot-auth-${loc.code}`)
	const dataDir = join(base, 'data')
	const configDir = join(base, 'config')
	// 每次重建为初始状态（清旧数据 + 隔离配置），保证可复现
	rmSync(base, { recursive: true, force: true })
	mkdirSync(dataDir, { recursive: true })
	makeDemoLibrary(dataDir, loc.code)

	const srv = startServer(loc.port, dataDir, configDir)
	try {
		await waitForServer(loc.port)
		await setPassword(loc.port)

		// 全新 context，localStorage 里没有 jasper.token → 匿名访问，撞上登录闸门。
		const ctx = await browser.newContext({
			viewport: VIEWPORT,
			deviceScaleFactor: SCALE,
			colorScheme: 'light',
			baseURL: `http://127.0.0.1:${loc.port}`,
		})
		await ctx.addInitScript(
			([code]) => {
				localStorage.setItem('jasper.locale', code)
				localStorage.setItem('jasper.theme', 'light')
			},
			[loc.code],
		)

		const page = await ctx.newPage()
		await page.goto('/')
		await page.locator('.locked-gate').waitFor({ state: 'visible' })
		await page.evaluate(() => document.fonts?.ready).catch(() => {})
		await page.waitForTimeout(250)
		await page.mouse.move(2, 2) // 移开鼠标，避免按钮 hover 高亮
		await page.screenshot({
			path: join(OUT_DIR, `06-access-control${loc.suffix}.png`),
			animations: 'disabled',
		})

		await ctx.close()
		console.log(`[shoot-auth] ${loc.code} ✓`)
	} finally {
		srv.kill('SIGTERM')
	}
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
	console.log('[shoot-auth] done →', OUT_DIR)
}

main().catch((e) => {
	console.error(e)
	process.exit(1)
})
