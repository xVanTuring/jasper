// 插件前端宿主：插件列表状态（rune）+ 主题贡献注入（<link>）+ 管理动作。
// 服务端未编译 plugins feature / demo 构建时整体不可用（pluginsAvailable() === false）。

import {
	api,
	IS_DEMO,
	type LocaleContribution,
	type PluginInfo,
	type PluginInstallResult,
	type SidebarContribution,
	type StorageContribution,
} from './api'
import { registerPluginThemes } from './theme.svelte'
import { registerPluginLocales, type PluginLocale } from './i18n.svelte'
import type { MsgKey } from './messages'

/** 设置向导可选的存储 provider（enabled 且无 error 的插件的 contributes.storage 摊平）。 */
export interface StorageProvider {
	pluginId: string
	pluginVersion: string
	contribution: StorageContribution
}

let available = $state(false)
let loaded = $state(false)
let plugins = $state<PluginInfo[]>([])

export function pluginsAvailable(): boolean {
	return available
}

/** 是否已完成首次探测（避免探测前 UI 抖动）。 */
export function pluginsLoaded(): boolean {
	return loaded
}

export function pluginList(): PluginInfo[] {
	return plugins
}

export function storageProviders(): StorageProvider[] {
	return plugins
		.filter((p) => p.enabled && !p.error)
		.flatMap((p) =>
			p.contributes.storage.map((s) => ({
				pluginId: p.id,
				pluginVersion: p.version,
				contribution: s,
			})),
		)
}

/** 侧边栏面板入口（enabled 且无 error 的插件的 contributes.sidebar 摊平，spec §3.5）。 */
export interface SidebarEntry {
	pluginId: string
	pluginName: string
	contribution: SidebarContribution
}

export function sidebarContributions(): SidebarEntry[] {
	return plugins
		.filter((p) => p.enabled && !p.error)
		.flatMap((p) =>
			p.contributes.sidebar.map((s) => ({
				pluginId: p.id,
				pluginName: p.name,
				contribution: s,
			})),
		)
}

/** 放到源码编辑器工具栏（note-toolbar）的插件 backend 命令。 */
export interface EditorCommand {
	pluginId: string
	commandId: string
	title: string
	icon: string
}

export function editorCommands(): EditorCommand[] {
	const out: EditorCommand[] = []
	for (const p of plugins) {
		if (!p.enabled || p.error) continue
		for (const tb of p.contributes.toolbar) {
			if (tb.location !== 'note-toolbar') continue
			const cmd = p.contributes.command.find((c) => c.id === tb.command && c.target === 'backend')
			if (cmd) out.push({ pluginId: p.id, commandId: cmd.id, title: cmd.title, icon: cmd.icon || 'rich' })
		}
	}
	return out
}

/**
 * 声明了 input 相位编辑器钩子的插件 id（enabled 且无 error）。源码编辑器在 debounce 输入后
 * 依次调它们的 editor.transform 改写缓冲（spec §3.7）。多个插件按加载顺序串联。
 */
export function editorInputPlugins(): string[] {
	return plugins
		// editor 是 0.4 新键：老宿主/伪造响应可能缺它 → `?? []` 兜底，绝不因缺键抛错
		.filter((p) => p.enabled && !p.error && (p.contributes.editor ?? []).some((e) => e.on === 'input'))
		.map((p) => p.id)
}

/** 探测 + 拉取插件列表；同步主题 <link> 注入。demo 构建直接视为不可用。 */
export async function loadPlugins(): Promise<void> {
	if (IS_DEMO) {
		loaded = true
		return
	}
	const resp = await api.plugins()
	available = resp !== null
	plugins = resp?.plugins ?? []
	loaded = true
	syncThemeLinks()
	// 插件主题登记进主题选择器；也是「所选主题的插件已消失 → 回落 auto」的收敛点
	registerPluginThemes(
		pluginThemes().map((t) => ({ id: t.id, name: t.name, base: t.base })),
	)
	// 插件语言包 fetch 目录 + 登记进 i18n（切换器多出该语言；来源消失时回落浏览器默认语言）
	await syncPluginLocales()
}

// ---------- 插件语言包（spec §3.10）----------
// enabled 插件的每个 locale 贡献 = 一份 catalog JSON（message key → 译文），
// 经资产端点 fetch 后登记进 i18n。fetch/解析失败的单条跳过（不拖垮其它语言）。

/** 当前应在场的语言贡献（含未选中的）。 */
function localeContributions(): { pluginId: string; version: string; contribution: LocaleContribution }[] {
	return plugins
		.filter((p) => p.enabled && !p.error)
		.flatMap((p) =>
			(p.contributes.locale ?? []).map((l) => ({
				pluginId: p.id,
				version: p.version,
				contribution: l,
			})),
		)
}

async function syncPluginLocales(): Promise<void> {
	const wanted = localeContributions()
	const out: PluginLocale[] = []
	await Promise.all(
		wanted.map(async ({ pluginId, version, contribution }) => {
			try {
				const url = api.pluginAssetUrl(pluginId, contribution.messages, version)
				const resp = await fetch(url)
				if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
				const raw: unknown = await resp.json()
				if (typeof raw !== 'object' || raw === null) throw new Error('catalog 不是对象')
				const table: Partial<Record<MsgKey, string>> = {}
				for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
					if (typeof v === 'string') table[k as MsgKey] = v
				}
				out.push({
					code: contribution.code,
					name: contribution.name,
					base: contribution.base === 'zh' ? 'zh' : 'en',
					messages: table,
				})
			} catch (e) {
				console.warn(`plugin locale "${contribution.code}" (${pluginId}) 加载失败`, e)
			}
		}),
	)
	registerPluginLocales(out)
}

export async function installPlugin(file: Blob, force = false): Promise<PluginInstallResult> {
	const r = await api.installPlugin(file, force)
	await loadPlugins()
	return r
}

export async function uninstallPlugin(id: string): Promise<void> {
	await api.deletePlugin(id)
	await loadPlugins()
}

export async function setPluginEnabled(id: string, enabled: boolean): Promise<void> {
	await api.setPluginEnabled(id, enabled)
	await loadPlugins()
}

// ---------- 插件主题注入 ----------
// enabled 插件的每个 theme 贡献 = 一条 <link rel="stylesheet">（css 走资产端点，?v=版本）。
// 主题的选中/回落逻辑在 theme.svelte.ts；这里只保证 CSS 在场。

const LINK_MARK = 'data-jasper-plugin-theme'

function linkIdOf(pluginId: string, themeId: string): string {
	return `plugin-theme-${pluginId}-${themeId}`
}

/** 当前应在场的主题贡献（含未选中的——切主题即时生效，无需再请求）。 */
export function pluginThemes(): { pluginId: string; id: string; name: string; base: 'light' | 'dark' }[] {
	return plugins
		.filter((p) => p.enabled && !p.error)
		.flatMap((p) => p.contributes.theme.map((t) => ({ pluginId: p.id, id: t.id, name: t.name, base: t.base })))
}

function syncThemeLinks(): void {
	if (typeof document === 'undefined') return
	const want = new Map<string, string>()
	for (const p of plugins) {
		if (!p.enabled || p.error) continue
		for (const th of p.contributes.theme) {
			want.set(linkIdOf(p.id, th.id), api.pluginAssetUrl(p.id, th.css, p.version))
		}
	}
	for (const el of Array.from(document.querySelectorAll(`link[${LINK_MARK}]`))) {
		const link = el as HTMLLinkElement
		const target = want.get(link.id)
		if (!target) {
			link.remove()
		} else {
			if (link.getAttribute('href') !== target) link.setAttribute('href', target)
			want.delete(link.id)
		}
	}
	for (const [id, href] of want) {
		const link = document.createElement('link')
		link.rel = 'stylesheet'
		link.id = id
		link.setAttribute(LINK_MARK, '')
		link.href = href
		document.head.appendChild(link)
	}
}
