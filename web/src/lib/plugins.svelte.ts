// 插件前端宿主：插件列表状态（rune）+ 主题贡献注入（<link>）+ 管理动作。
// 服务端未编译 plugins feature / demo 构建时整体不可用（pluginsAvailable() === false）。

import {
	api,
	IS_DEMO,
	type PluginInfo,
	type PluginInstallResult,
	type SidebarContribution,
	type StorageContribution,
} from './api'
import { registerPluginThemes } from './theme.svelte'

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
