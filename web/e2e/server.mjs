// Playwright webServer 启动器：重建临时数据源 + 隔离配置目录，然后起真正的 Rust 后端。
// 环境变量由 playwright.config.ts 通过 webServer.env 传入。
import { existsSync, rmSync } from 'node:fs'
import { spawn } from 'node:child_process'
import { makeFixture } from './make-fixture.mjs'

const {
	JASPER_SOURCE, // = DATA_DIR
	JASPER_CONFIG_DIR, // = CONFIG_DIR
	JASPER_BIN, // 后端二进制
	JASPER_WEB_DIR, // 已构建前端
} = process.env

if (!existsSync(JASPER_BIN)) {
	console.error(
		`\n[e2e] 找不到后端二进制：${JASPER_BIN}\n先构建：cd server && cargo build（或设 JASPER_BIN）\n`,
	)
	process.exit(1)
}
if (!existsSync(JASPER_WEB_DIR)) {
	console.error(`\n[e2e] 找不到前端产物：${JASPER_WEB_DIR}\n先构建：cd web && pnpm build\n`)
	process.exit(1)
}

// 每次起服务都重建为初始状态，保证测试可复现
rmSync(JASPER_SOURCE, { recursive: true, force: true })
rmSync(JASPER_CONFIG_DIR, { recursive: true, force: true })
makeFixture(JASPER_SOURCE)

const child = spawn(JASPER_BIN, [], {
	env: process.env, // 已含 JASPER_SOURCE / JASPER_CONFIG_DIR / JASPER_HOST / JASPER_PORT / JASPER_WEB_DIR
	stdio: 'inherit',
})

const bye = () => child.killed || child.kill('SIGTERM')
process.on('SIGTERM', bye)
process.on('SIGINT', bye)
process.on('exit', bye)
child.on('exit', (code) => process.exit(code ?? 0))
