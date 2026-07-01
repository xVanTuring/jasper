// e2e 共享常量：端口、临时数据/配置目录、二进制与前端目录路径。
// playwright.config.ts 用它拼 webServer.env；各 spec 用它读回写后的 .md 文件。
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

export const PORT = Number(process.env.JASPER_E2E_PORT ?? 27599)
export const BASE_URL = `http://127.0.0.1:${PORT}`

// 固定的临时目录（每次起服务前重建），便于 spec 直接按路径读回写后的文件。
const ROOT = join(tmpdir(), 'jasper-e2e')
export const DATA_DIR = join(ROOT, 'data') // 数据源（Joplin 库）
export const CONFIG_DIR = join(ROOT, 'config') // config.db / cache.db 隔离

// 相对 web/（pnpm e2e 的 cwd）解析后端二进制与已构建前端。可用 JASPER_BIN 覆盖。
export const SERVER_BIN =
	process.env.JASPER_BIN ?? resolve(process.cwd(), '../server/target/debug/jasper')
export const WEB_DIR = resolve(process.cwd(), 'dist')
