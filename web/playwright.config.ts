import { defineConfig, devices } from '@playwright/test'
import { BASE_URL, PORT, DATA_DIR, CONFIG_DIR, SERVER_BIN, WEB_DIR } from './e2e/config'

// e2e 全在本机；若环境设了 HTTP(S)_PROXY，Playwright 的健康检查会把 127.0.0.1 也经代理
// → 连不上。把本地地址加入 NO_PROXY，确保直连。
const noProxy = ['127.0.0.1', 'localhost', process.env.NO_PROXY, process.env.no_proxy]
	.filter(Boolean)
	.join(',')
process.env.NO_PROXY = noProxy
process.env.no_proxy = noProxy

// 全栈 e2e：启动器起真正的 Rust 后端（指向临时 Joplin 库），浏览器驱动真前端。
// 后端是有状态的（写盘），故串行、单 worker，避免用例互相污染。
export default defineConfig({
	testDir: './e2e',
	fullyParallel: false,
	workers: 1,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	reporter: process.env.CI ? [['github'], ['list']] : [['list']],
	use: {
		baseURL: BASE_URL,
		trace: 'on-first-retry',
	},
	// 默认用 Playwright 自带 chromium（CI）；本地无法下载时可设 E2E_CHANNEL=chrome 用系统 Chrome。
	projects: [
		{
			name: 'chromium',
			use: { ...devices['Desktop Chrome'], channel: process.env.E2E_CHANNEL || undefined },
		},
	],
	webServer: {
		command: 'node e2e/server.mjs',
		url: `${BASE_URL}/api/status`,
		timeout: 60_000,
		reuseExistingServer: !process.env.CI,
		stdout: 'pipe', // 把后端日志透传到测试输出，便于排查
		stderr: 'pipe',
		// 注意：必须并入 process.env（否则 PATH 被清空，连 node 都找不到）
		env: {
			...process.env,
			JASPER_SOURCE: DATA_DIR,
			JASPER_CONFIG_DIR: CONFIG_DIR,
			JASPER_BIN: SERVER_BIN,
			JASPER_WEB_DIR: WEB_DIR,
			JASPER_HOST: '127.0.0.1',
			JASPER_PORT: String(PORT),
		},
	},
})
