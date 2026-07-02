// 插件市场 rune store：索引拉取 + 条目状态推导 + 「下载→sha256 校验→装进宿主」动作。
// 纯逻辑（解析/比较/兼容）在 market.ts；这里只管状态与副作用。

import { api, type PluginInfo } from './api'
import {
	MARKET_INDEX_URL,
	compat,
	cmpVersion,
	parseIndex,
	sha256Hex,
	type Compat,
	type MarketEntry,
} from './market'
import { loadPlugins } from './plugins.svelte'

let entries = $state<MarketEntry[]>([])
let hostVersion = $state('')
let loading = $state(false)
let loadError = $state('')
let loadedOnce = $state(false)

export function marketEntries(): MarketEntry[] {
	return entries
}
export function marketLoading(): boolean {
	return loading
}
export function marketError(): string {
	return loadError
}
export function marketLoadedOnce(): boolean {
	return loadedOnce
}

/** 拉索引 + 宿主版本（首次打开市场 tab 时调用；force 供「重试/刷新」）。 */
export async function loadMarket(force = false): Promise<void> {
	if (loading || (loadedOnce && !force && !loadError)) return
	loading = true
	loadError = ''
	try {
		const [res, status] = await Promise.all([
			fetch(MARKET_INDEX_URL, { cache: 'no-cache' }),
			api.status(),
		])
		if (!res.ok) throw new Error(`registry -> HTTP ${res.status}`)
		entries = parseIndex(await res.json())
		hostVersion = status.version
		loadedOnce = true
	} catch (e) {
		loadError = e instanceof Error ? e.message : `${e}`
	} finally {
		loading = false
	}
}

export type EntryState =
	| { kind: 'incompatible'; compat: Extract<Compat, { ok: false }> }
	| { kind: 'not_installed' }
	| { kind: 'installed' }
	| { kind: 'update'; installedVersion: string }

/** 条目相对本机的状态（不兼容 > 可更新 > 已装 > 未装）。 */
export function entryState(entry: MarketEntry, installed: PluginInfo[]): EntryState {
	const c = compat(entry, hostVersion)
	if (!c.ok) return { kind: 'incompatible', compat: c }
	const local = installed.find((p) => p.id === entry.id)
	if (!local) return { kind: 'not_installed' }
	if (cmpVersion(entry.version, local.version) > 0) {
		return { kind: 'update', installedVersion: local.version }
	}
	return { kind: 'installed' }
}

/**
 * 从市场安装：浏览器下载 .jplug → sha256 对索引校验 → POST 给宿主安装端点。
 * 校验失败抛错并中止（不把可疑字节交给宿主）。返回宿主安装结果（含 needs_consent）。
 */
export async function installFromMarket(entry: MarketEntry, force = false) {
	const res = await fetch(entry.download)
	if (!res.ok) throw new Error(`download -> HTTP ${res.status}`)
	const bytes = await res.arrayBuffer()
	const digest = await sha256Hex(bytes)
	if (digest !== entry.sha256) {
		throw new Error(`sha256 mismatch: expected ${entry.sha256}, got ${digest}`)
	}
	const r = await api.installPlugin(new Blob([bytes], { type: 'application/zip' }), force)
	await loadPlugins()
	return r
}
