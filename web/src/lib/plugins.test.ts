// plugins.svelte.ts：探测（含 SPA-fallback 坑）、provider 过滤、主题 <link> 注入。
import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PluginContributes, PluginInfo } from './api'

function pluginInfo(
	over: Partial<Omit<PluginInfo, 'contributes'>> & { contributes?: Partial<PluginContributes> },
): PluginInfo {
	const { contributes, ...rest } = over
	return {
		id: 'p',
		name: 'P',
		version: '1.0.0',
		api_version: '0.2',
		description: '',
		author: '',
		enabled: true,
		has_backend: false,
		capabilities: [],
		hooks: [],
		error: null,
		// 全键默认 + 按需覆盖（测试只需给关心的贡献键，其余含 editor 默认空数组）
		contributes: { theme: [], locale: [], storage: [], command: [], toolbar: [], sidebar: [], editor: [], ...contributes },
		settings_schema: {},
		write_auto_approve: false,
		...rest,
	}
}

function mockPluginsResp(plugins: PluginInfo[]) {
	return new Response(JSON.stringify({ host: { version: '0.1.0', api_versions: ['0.1', '0.2'] }, plugins }), {
		status: 200,
		headers: { 'content-type': 'application/json' },
	})
}

async function freshStore() {
	vi.resetModules() // 模块级 rune 状态按用例隔离
	return import('./plugins.svelte')
}

beforeEach(() => {
	document.head.querySelectorAll('link[data-jasper-plugin-theme]').forEach((el) => el.remove())
})

describe('loadPlugins 探测', () => {
	it('JSON 200 → 可用并暴露列表', async () => {
		vi.stubGlobal('fetch', vi.fn(async () => mockPluginsResp([pluginInfo({ id: 'a' })])))
		const store = await freshStore()
		await store.loadPlugins()
		expect(store.pluginsAvailable()).toBe(true)
		expect(store.pluginList().map((p) => p.id)).toEqual(['a'])
	})

	it('SPA fallback（HTML 200）→ 不可用', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () => new Response('<!doctype html>', { status: 200, headers: { 'content-type': 'text/html' } })),
		)
		const store = await freshStore()
		await store.loadPlugins()
		expect(store.pluginsAvailable()).toBe(false)
		expect(store.pluginsLoaded()).toBe(true)
	})

	it('网络失败 → 不可用且不抛', async () => {
		vi.stubGlobal('fetch', vi.fn(async () => Promise.reject(new Error('boom'))))
		const store = await freshStore()
		await store.loadPlugins()
		expect(store.pluginsAvailable()).toBe(false)
	})
})

describe('storageProviders', () => {
	it('只取 enabled 且无 error 的插件的 storage 贡献', async () => {
		const storage = { id: 's', name: 'S', icon: '', config_schema: {} }
		vi.stubGlobal(
			'fetch',
			vi.fn(async () =>
				mockPluginsResp([
					pluginInfo({ id: 'ok', contributes: { theme: [], locale: [], storage: [storage], command: [], toolbar: [], sidebar: [] } }),
					pluginInfo({ id: 'off', enabled: false, contributes: { theme: [], locale: [], storage: [storage], command: [], toolbar: [], sidebar: [] } }),
					pluginInfo({ id: 'bad', error: 'x', contributes: { theme: [], locale: [], storage: [storage], command: [], toolbar: [], sidebar: [] } }),
				]),
			),
		)
		const store = await freshStore()
		await store.loadPlugins()
		expect(store.storageProviders().map((p) => p.pluginId)).toEqual(['ok'])
	})
})

describe('editorCommands', () => {
	it('取 enabled 插件的 note-toolbar backend 命令', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () =>
				mockPluginsResp([
					pluginInfo({
						id: 'ai',
						contributes: {
							theme: [], locale: [],
							storage: [],
							command: [{ id: 'polish', title: '优化', icon: 'rich', target: 'backend' }],
							toolbar: [{ command: 'polish', location: 'note-toolbar' }],
							sidebar: [],
						},
					}),
					// 禁用的插件不计入
					pluginInfo({
						id: 'off',
						enabled: false,
						contributes: {
							theme: [], locale: [],
							storage: [],
							command: [{ id: 'x', title: 'X', icon: '', target: 'backend' }],
							toolbar: [{ command: 'x', location: 'note-toolbar' }],
							sidebar: [],
						},
					}),
					// topbar 位置不进编辑器工具栏
					pluginInfo({
						id: 'top',
						contributes: {
							theme: [], locale: [],
							storage: [],
							command: [{ id: 'y', title: 'Y', icon: '', target: 'backend' }],
							toolbar: [{ command: 'y', location: 'topbar' }],
							sidebar: [],
						},
					}),
				]),
			),
		)
		const store = await freshStore()
		await store.loadPlugins()
		const cmds = store.editorCommands()
		expect(cmds.map((c) => c.pluginId)).toEqual(['ai'])
		expect(cmds[0]).toMatchObject({ commandId: 'polish', title: '优化', icon: 'rich' })
	})
})

describe('editorInputPlugins', () => {
	it('只取 enabled 且无 error、声明了 input 相位 editor 钩子的插件 id', async () => {
		vi.stubGlobal(
			'fetch',
			vi.fn(async () =>
				mockPluginsResp([
					pluginInfo({ id: 'fmt', contributes: { editor: [{ on: 'input' }] } }),
					// 只声明 before-save 相位 → 不进 input 列表
					pluginInfo({ id: 'save-only', contributes: { editor: [{ on: 'before-save' }] } }),
					// 禁用 / 出错的不计入
					pluginInfo({ id: 'off', enabled: false, contributes: { editor: [{ on: 'input' }] } }),
					pluginInfo({ id: 'bad', error: 'x', contributes: { editor: [{ on: 'input' }] } }),
				]),
			),
		)
		const store = await freshStore()
		await store.loadPlugins()
		expect(store.editorInputPlugins()).toEqual(['fmt'])
	})
})

describe('sidebarContributions', () => {
	it('只取 enabled 且无 error 的插件的 sidebar 贡献（含插件名）', async () => {
		const sidebar = { id: 'chat-panel', title: 'AI 对话', icon: 'chat', widget: 'chat', command: 'chat' }
		vi.stubGlobal(
			'fetch',
			vi.fn(async () =>
				mockPluginsResp([
					pluginInfo({
						id: 'ok',
						name: 'AI 助手',
						contributes: { theme: [], locale: [], storage: [], command: [], toolbar: [], sidebar: [sidebar] },
					}),
					pluginInfo({
						id: 'off',
						enabled: false,
						contributes: { theme: [], locale: [], storage: [], command: [], toolbar: [], sidebar: [sidebar] },
					}),
					pluginInfo({
						id: 'bad',
						error: 'x',
						contributes: { theme: [], locale: [], storage: [], command: [], toolbar: [], sidebar: [sidebar] },
					}),
				]),
			),
		)
		const store = await freshStore()
		await store.loadPlugins()
		const entries = store.sidebarContributions()
		expect(entries.map((e) => e.pluginId)).toEqual(['ok'])
		expect(entries[0].pluginName).toBe('AI 助手')
		expect(entries[0].contribution.widget).toBe('chat')
	})
})

describe('主题 <link> 注入', () => {
	it('enabled 插件的主题注入、禁用后移除', async () => {
		const theme = { id: 'oceanic', name: 'Oceanic', base: 'dark' as const, css: 'assets/o.css' }
		const withTheme = [pluginInfo({ id: 'th', version: '2.0.0', contributes: { theme: [theme], locale: [], storage: [], command: [], toolbar: [], sidebar: [] } })]
		const fetchMock = vi.fn(async () => mockPluginsResp(withTheme))
		vi.stubGlobal('fetch', fetchMock)
		const store = await freshStore()
		await store.loadPlugins()

		const link = document.getElementById('plugin-theme-th-oceanic') as HTMLLinkElement
		expect(link).toBeTruthy()
		expect(link.getAttribute('href')).toBe('/api/plugins/th/assets/assets/o.css?v=2.0.0')

		// 禁用后（列表刷新）link 应被移除
		fetchMock.mockImplementation(async () => mockPluginsResp([{ ...withTheme[0], enabled: false }]))
		await store.loadPlugins()
		expect(document.getElementById('plugin-theme-th-oceanic')).toBeNull()
	})
})
