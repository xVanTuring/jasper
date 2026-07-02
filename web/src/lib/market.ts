// 插件市场纯逻辑（无 rune、可单测）：索引解析 / 版本比较 / 兼容判断 / 双语取词 / sha256。
// 索引契约见 jasper-plugin-registry 仓库 README（schemaVersion 1，name/description 为 {zh,en} 双语对象）。

/** 市场索引地址（raw.githubusercontent 带 CORS，浏览器可直接 fetch）。 */
export const MARKET_INDEX_URL =
	'https://raw.githubusercontent.com/xVanTuring/jasper-plugin-registry/main/plugins.json'

/** 宿主支持的插件 API 大版本（镜像 server/src/plugins/manifest.rs 的 HOST_API_VERSIONS）。 */
export const HOST_PLUGIN_API_MAJORS = ['0']

export interface LocalizedText {
	zh: string
	en: string
}

export interface MarketEntry {
	id: string
	name: LocalizedText
	description: LocalizedText
	author: string
	repo: string
	version: string
	apiVersion: string
	minHostVersion: string
	capabilities: string[]
	download: string
	sha256: string
}

function isLocalized(v: unknown): v is LocalizedText {
	if (typeof v !== 'object' || v === null) return false
	const o = v as Record<string, unknown>
	return typeof o.zh === 'string' && o.zh.trim() !== '' && typeof o.en === 'string' && o.en.trim() !== ''
}

/**
 * 解析索引 JSON。schemaVersion 不认识 → 抛错（让 UI 提示升级）；
 * 单条不合法 → 跳过并 console.warn（索引前向兼容，坏一条不坏整页）。
 */
export function parseIndex(data: unknown): MarketEntry[] {
	const root = data as { schemaVersion?: unknown; plugins?: unknown }
	if (root?.schemaVersion !== 1) {
		throw new Error(`unsupported registry schemaVersion: ${String(root?.schemaVersion)}`)
	}
	if (!Array.isArray(root.plugins)) return []
	const out: MarketEntry[] = []
	for (const raw of root.plugins) {
		const e = raw as Record<string, unknown>
		const ok =
			typeof e.id === 'string' &&
			/^[a-z0-9][a-z0-9-]*$/.test(e.id) &&
			isLocalized(e.name) &&
			isLocalized(e.description) &&
			typeof e.version === 'string' &&
			typeof e.apiVersion === 'string' &&
			Array.isArray(e.capabilities) &&
			e.capabilities.every((c) => typeof c === 'string') &&
			typeof e.download === 'string' &&
			e.download.startsWith('https://') &&
			typeof e.sha256 === 'string' &&
			/^[0-9a-f]{64}$/.test(e.sha256)
		if (!ok) {
			console.warn('market: skipping malformed registry entry', raw)
			continue
		}
		out.push({
			id: e.id as string,
			name: e.name as LocalizedText,
			description: e.description as LocalizedText,
			author: typeof e.author === 'string' ? e.author : '',
			repo: typeof e.repo === 'string' ? e.repo : '',
			version: e.version as string,
			apiVersion: e.apiVersion as string,
			minHostVersion: typeof e.minHostVersion === 'string' ? e.minHostVersion : '',
			capabilities: e.capabilities as string[],
			download: e.download as string,
			sha256: (e.sha256 as string).toLowerCase(),
		})
	}
	return out
}

/** 版本比较（移植 manifest.rs cmp_version）：点分段，数字段按数值、非数字段按字符串，缺段视为 0。 */
export function cmpVersion(a: string, b: string): number {
	const sa = a.trim().split('.')
	const sb = b.trim().split('.')
	const n = Math.max(sa.length, sb.length)
	for (let i = 0; i < n; i++) {
		const x = sa[i] ?? '0'
		const y = sb[i] ?? '0'
		const nx = /^\d+$/.test(x) ? parseInt(x, 10) : null
		const ny = /^\d+$/.test(y) ? parseInt(y, 10) : null
		let c: number
		if (nx !== null && ny !== null) c = nx === ny ? 0 : nx < ny ? -1 : 1
		else c = x === y ? 0 : x < y ? -1 : 1
		if (c !== 0) return c
	}
	return 0
}

export type Compat = { ok: true } | { ok: false; reason: 'api' | 'host' }

/** 兼容判断：apiVersion 大版本须在宿主支持集；minHostVersion ≤ 宿主版本。 */
export function compat(entry: Pick<MarketEntry, 'apiVersion' | 'minHostVersion'>, hostVersion: string): Compat {
	const major = entry.apiVersion.split('.')[0]
	if (!HOST_PLUGIN_API_MAJORS.includes(major)) return { ok: false, reason: 'api' }
	if (entry.minHostVersion.trim() !== '' && cmpVersion(entry.minHostVersion, hostVersion) > 0) {
		return { ok: false, reason: 'host' }
	}
	return { ok: true }
}

/** 按 UI 语言取词；单键缺失时回落另一语言（parseIndex 已保证两键都在，防御性兜底）。 */
export function pickText(text: LocalizedText, lang: string): string {
	return lang === 'zh' ? text.zh || text.en : text.en || text.zh
}

/** sha256 十六进制（WebCrypto；localhost 是 secure context）。 */
export async function sha256Hex(bytes: ArrayBuffer): Promise<string> {
	const digest = await crypto.subtle.digest('SHA-256', bytes)
	return Array.from(new Uint8Array(digest))
		.map((b) => b.toString(16).padStart(2, '0'))
		.join('')
}
